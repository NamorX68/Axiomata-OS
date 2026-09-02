//! The routine poll loop.
//!
//! A single background Tokio task ([`spawn`]) wakes every [`POLL_INTERVAL`],
//! asks [`crate::routines::store`] which routines are due, and fires each one
//! through the same agent path a manual skill run uses. Every firing writes a
//! `routine_runs` row (and, when an agent actually ran, a `runs` row it links
//! to) and advances the routine's persisted `next_fire_at`.
//!
//! One pass is [`tick`], which is public so the CLI (`axiomata-cli routines
//! tick`) and tests can drive it without waiting on the timer.
//!
//! ## Restart safety
//!
//! `next_fire_at` is read from the database, never recomputed from the cron
//! expression on startup. Before the loop starts, [`spawn`] runs one
//! reconciliation pass: any routine whose `next_fire_at` is already in the
//! past (the app was off when it was due) is rolled forward to its next
//! occurrence and gets a `missed` history row — it does **not** fire to catch
//! up. So restarting can neither double-fire a routine nor lose one.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;

use crate::agents::AgentBackend;
use crate::config::Config;
use crate::error::AxiomataError;
use crate::routines::model::{Routine, RoutineRunStatus, RoutineTarget};
use crate::routines::store::NewRoutineRun;
use crate::routines::{schedule, store};
use crate::skills::model::{RunRecord, RunStatus};
use crate::skills::{runlog, runner};

/// How often the loop wakes to check for due routines. A routine fires within
/// this long of its scheduled instant.
pub const POLL_INTERVAL: Duration = Duration::from_secs(30);

/// Backend used for a `prompt` routine that does not name one itself. Local
/// and dependency-free, matching the "daily digest against a local model"
/// use case.
const DEFAULT_PROMPT_BACKEND: &str = "ollama";

/// What one [`tick`] did.
#[derive(Debug, Default, Clone, Serialize)]
pub struct TickReport {
    /// Routines that fired this pass (regardless of outcome).
    pub fired: usize,
    /// Of those, how many the target reported success for.
    pub succeeded: usize,
    /// Of those, how many failed (target error, missing skill, non-zero exit).
    pub failed: usize,
    /// Routines that could not be processed at all this pass (e.g. a database
    /// write failed); `"<name>: <error>"`. Their `next_fire_at` is left
    /// unchanged, so the next tick retries them.
    pub errors: Vec<String>,
}

/// Runs one poll pass: fire every enabled routine whose `next_fire_at` is at
/// or before now, then advance each one to its next occurrence.
///
/// A routine that fires exactly once here even if several of its scheduled
/// slots have elapsed — `next_fire_at` jumps to the next occurrence strictly
/// after now, never replaying the backlog.
///
/// # Errors
///
/// Returns [`AxiomataError::Database`] only if the initial "what is due" query
/// fails. A failure while firing an individual routine is collected into
/// [`TickReport::errors`], not propagated, so one bad routine cannot stop the
/// others.
pub async fn tick(
    config: &Config,
    db: &Arc<Mutex<Connection>>,
) -> Result<TickReport, AxiomataError> {
    let now = Utc::now();
    let due = {
        let conn = lock(db);
        store::due_routines(&conn, now)?
    };

    let mut report = TickReport::default();
    for routine in due {
        match fire_one(&routine, config, db, now).await {
            Ok(RoutineRunStatus::Success) => {
                report.fired += 1;
                report.succeeded += 1;
            }
            Ok(_) => {
                report.fired += 1;
                report.failed += 1;
            }
            Err(err) => report.errors.push(format!("{}: {err}", routine.name)),
        }
    }
    Ok(report)
}

/// Executes one routine's target, records the outcome in `runs` (when an agent
/// ran) and `routine_runs`, and advances `next_fire_at`.
///
/// Returns the recorded [`RoutineRunStatus`]. Returns `Err` only if a database
/// write fails; in that case `next_fire_at` is deliberately left as-is so the
/// routine is retried on the next tick.
async fn fire_one(
    routine: &Routine,
    config: &Config,
    db: &Arc<Mutex<Connection>>,
    fired_at: DateTime<Utc>,
) -> Result<RoutineRunStatus, AxiomataError> {
    let scheduled_for = routine.next_fire_at.unwrap_or(fired_at);

    // The agent call: no lock held (it awaits, possibly for the whole timeout).
    let outcome = execute_target(routine, config).await;

    // Next occurrence strictly after now — computed once, before taking the
    // lock. `validate` ran at creation time, so a parse error here means the
    // stored row was corrupted; surface it rather than firing forever.
    let next_fire_at = schedule::next_after(&routine.cron_expr, Utc::now())?;

    let conn = lock(db);
    let (status, detail, run_id) = match &outcome {
        Ok(record) => {
            let stored = runlog::record_run(&conn, record.clone())?;
            let status = if record.status == RunStatus::Success {
                RoutineRunStatus::Success
            } else {
                RoutineRunStatus::Failed
            };
            (status, record.error.clone(), stored.id)
        }
        Err(err) => (RoutineRunStatus::Failed, Some(err.to_string()), None),
    };

    store::record_run(
        &conn,
        routine.id,
        NewRoutineRun {
            run_id,
            scheduled_for,
            fired_at,
            status,
            detail,
        },
    )?;
    store::mark_fired(&conn, routine.id, fired_at, next_fire_at)?;
    Ok(status)
}

/// Dispatches to the skill runner or the raw-prompt path.
///
/// `Err` here means the target could not be executed at all (a skill routine
/// whose skill no longer exists); every other outcome, including a non-zero
/// exit, is an `Ok(RunRecord)`.
async fn execute_target(routine: &Routine, config: &Config) -> Result<RunRecord, AxiomataError> {
    match &routine.target {
        RoutineTarget::Skill(name) => runner::execute_skill(name, config).await,
        RoutineTarget::Prompt(text) => Ok(run_prompt(routine, text, config).await),
    }
}

/// Runs a raw prompt against the routine's backend (or [`DEFAULT_PROMPT_BACKEND`]),
/// always returning a [`RunRecord`] — an unresolvable backend string becomes a
/// `Failed` record rather than an error.
async fn run_prompt(routine: &Routine, prompt: &str, config: &Config) -> RunRecord {
    let started_at = Utc::now();
    let backend_id = routine.backend.as_deref().unwrap_or(DEFAULT_PROMPT_BACKEND);

    let backend = match AgentBackend::resolve(backend_id, None, config) {
        Ok(backend) => backend,
        Err(err) => {
            return runner::failure_record(
                &routine.name,
                backend_id,
                started_at,
                0,
                err.to_string(),
            );
        }
    };

    let request = runner::agent_request(prompt.to_owned(), &backend, config);
    match backend.run(request).await {
        Ok(result) => runner::record_from_result(&routine.name, backend.id(), started_at, result),
        Err(err) => {
            let elapsed = (Utc::now() - started_at).num_milliseconds().max(0) as u64;
            runner::failure_record(
                &routine.name,
                backend.id(),
                started_at,
                elapsed,
                err.to_string(),
            )
        }
    }
}

/// Rolls every already-past routine forward without firing it, recording a
/// `missed` history row for each. Run once at startup, before the loop.
///
/// Returns how many routines were rolled forward.
async fn reconcile_missed(
    config: &Config,
    db: &Arc<Mutex<Connection>>,
) -> Result<usize, AxiomataError> {
    let _ = config;
    let now = Utc::now();
    let stale = {
        let conn = lock(db);
        store::due_routines(&conn, now)?
    };

    let mut rolled = 0;
    for routine in stale {
        let scheduled_for = routine.next_fire_at.unwrap_or(now);
        let next_fire_at = schedule::next_after(&routine.cron_expr, now)?;

        let conn = lock(db);
        store::record_run(
            &conn,
            routine.id,
            NewRoutineRun {
                run_id: None,
                scheduled_for,
                fired_at: now,
                status: RoutineRunStatus::Missed,
                detail: Some("app was not running at the scheduled time".to_owned()),
            },
        )?;
        store::roll_forward(&conn, routine.id, next_fire_at)?;
        rolled += 1;
    }
    Ok(rolled)
}

/// A running scheduler loop. Dropping it, or calling [`SchedulerHandle::shutdown`],
/// stops the loop after its current tick.
pub struct SchedulerHandle {
    stop: tokio::sync::watch::Sender<bool>,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl SchedulerHandle {
    /// Signals the loop to stop and waits for the task to finish.
    pub async fn shutdown(mut self) {
        let _ = self.stop.send(true);
        if let Some(join) = self.join.take() {
            let _ = join.await;
        }
    }
}

impl Drop for SchedulerHandle {
    fn drop(&mut self) {
        // Best-effort: tell the loop to stop. If the caller used `shutdown`
        // the join already happened; otherwise the task ends on its own at the
        // next tick boundary.
        let _ = self.stop.send(true);
    }
}

/// Starts the routine scheduler on the current Tokio runtime.
///
/// Takes owned copies of the config and a shared handle to the database so the
/// task outlives the caller. The returned [`SchedulerHandle`] must be kept
/// alive for the loop to keep running.
pub fn spawn(config: Config, db: Arc<Mutex<Connection>>) -> SchedulerHandle {
    spawn_with_interval(config, db, POLL_INTERVAL)
}

/// [`spawn`] with a caller-chosen poll interval — used by tests to drive the
/// loop without waiting [`POLL_INTERVAL`].
pub(crate) fn spawn_with_interval(
    config: Config,
    db: Arc<Mutex<Connection>>,
    interval: Duration,
) -> SchedulerHandle {
    let (stop, mut stop_rx) = tokio::sync::watch::channel(false);

    let join = tokio::spawn(async move {
        match reconcile_missed(&config, &db).await {
            Ok(0) => {}
            Ok(n) => tracing::info!(rolled_forward = n, "routine scheduler skipped missed fires"),
            Err(err) => tracing::warn!(%err, "routine scheduler startup reconciliation failed"),
        }

        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = ticker.tick() => match tick(&config, &db).await {
                    Ok(report) if report.fired > 0 || !report.errors.is_empty() => {
                        tracing::info!(
                            fired = report.fired,
                            succeeded = report.succeeded,
                            failed = report.failed,
                            errors = ?report.errors,
                            "routine tick",
                        );
                    }
                    Ok(_) => {}
                    Err(err) => tracing::warn!(%err, "routine scheduler tick failed"),
                },
                result = stop_rx.changed() => {
                    // Sender dropped or told us to stop.
                    if result.is_err() || *stop_rx.borrow() {
                        break;
                    }
                }
            }
        }
    });

    SchedulerHandle {
        stop,
        join: Some(join),
    }
}

/// Locks the shared connection, treating poisoning (a previous panic while
/// holding it) as unrecoverable — consistent with the rest of the crate.
fn lock(db: &Arc<Mutex<Connection>>) -> std::sync::MutexGuard<'_, Connection> {
    db.lock()
        .expect("routine scheduler database mutex is poisoned")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routines::model::{NewRoutine, RoutineState};
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    /// Isolated home + workspace + migrated DB behind the shared `Arc<Mutex>`.
    struct Fixture {
        _guard: std::sync::MutexGuard<'static, ()>,
        home: PathBuf,
        workspace: PathBuf,
        config: Config,
        db: Arc<Mutex<Connection>>,
    }

    impl Fixture {
        fn new(tag: &str) -> Self {
            let guard = ENV_MUTEX.lock().unwrap();
            let home = unique_temp_dir(&format!("axiomata-test-sched-{tag}-home"));
            let workspace = unique_temp_dir(&format!("axiomata-test-sched-{tag}-ws"));
            fs::create_dir_all(home.join("logs")).unwrap();
            fs::create_dir_all(&workspace).unwrap();
            // SAFETY: serialized by `guard`, see `paths::tests`.
            unsafe {
                env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
            }
            let db = crate::db::open_and_migrate_at(&home.join("axiomata.db")).unwrap();
            let mut config = Config {
                workspace_root: workspace.clone(),
                ..Config::default()
            };
            config.agents.skill_timeout_secs = 5;
            Self {
                _guard: guard,
                home,
                workspace,
                config,
                db: Arc::new(Mutex::new(db)),
            }
        }

        fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
            self.db.lock().unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            unsafe {
                env::remove_var(crate::paths::AXIOMATA_HOME_ENV);
            }
            let _ = fs::remove_dir_all(&self.home);
            let _ = fs::remove_dir_all(&self.workspace);
        }
    }

    /// A prompt routine pointed at the (unreachable in tests) Ollama daemon, so
    /// firing produces a `Failed` run without needing a real agent — enough to
    /// exercise the scheduling machinery.
    fn prompt_routine(name: &str, cron: &str) -> NewRoutine {
        NewRoutine {
            name: name.to_owned(),
            cron_expr: cron.to_owned(),
            target: RoutineTarget::Prompt("say hello".to_owned()),
            backend: Some("ollama".to_owned()),
            enabled: true,
        }
    }

    #[tokio::test]
    async fn tick_fires_due_routine_records_history_and_advances_next_fire() {
        let fx = Fixture::new("tick");
        let routine = store::add(&fx.conn(), prompt_routine("greeter", "*/1 * * * * *")).unwrap();

        // Force it due.
        let past = Utc::now() - chrono::Duration::seconds(5);
        store::roll_forward(&fx.conn(), routine.id, Some(past)).unwrap();

        let report = tick(&fx.config, &fx.db).await.unwrap();
        assert_eq!(report.fired, 1);
        assert!(report.errors.is_empty());

        // History row written, linked run recorded, next fire moved to the future.
        let after = store::get(&fx.conn(), routine.id).unwrap().unwrap();
        assert!(after.last_fired_at.is_some());
        assert!(after.next_fire_at.unwrap() > Utc::now());

        let history = store::list_runs(&fx.conn(), routine.id, 10).unwrap();
        assert_eq!(history.len(), 1);
        assert_ne!(history[0].status, RoutineRunStatus::Missed);
        let state = RoutineState::derive(&after, history.first());
        assert!(matches!(state, RoutineState::Fired | RoutineState::Failed));

        // A run row exists (the Ollama failure is still a recorded run).
        assert_eq!(runlog::list_runs(&fx.conn(), 10).unwrap().len(), 1);

        // Second immediate tick does nothing: no longer due.
        let again = tick(&fx.config, &fx.db).await.unwrap();
        assert_eq!(again.fired, 0);
    }

    #[tokio::test]
    async fn tick_fires_each_due_routine_once_not_once_per_missed_slot() {
        let fx = Fixture::new("no-backlog");
        let routine =
            store::add(&fx.conn(), prompt_routine("every-second", "*/1 * * * * *")).unwrap();
        // Due since an hour ago — thousands of elapsed slots.
        let long_ago = Utc::now() - chrono::Duration::hours(1);
        store::roll_forward(&fx.conn(), routine.id, Some(long_ago)).unwrap();

        let report = tick(&fx.config, &fx.db).await.unwrap();
        assert_eq!(report.fired, 1);
        assert_eq!(
            store::list_runs(&fx.conn(), routine.id, 100).unwrap().len(),
            1
        );
    }

    #[tokio::test]
    async fn disabled_routine_is_never_fired() {
        let fx = Fixture::new("disabled");
        let mut new = prompt_routine("sleeper", "*/1 * * * * *");
        new.enabled = false;
        let routine = store::add(&fx.conn(), new).unwrap();
        store::roll_forward(
            &fx.conn(),
            routine.id,
            Some(Utc::now() - chrono::Duration::seconds(10)),
        )
        .unwrap();

        let report = tick(&fx.config, &fx.db).await.unwrap();
        assert_eq!(report.fired, 0);
        assert!(
            store::list_runs(&fx.conn(), routine.id, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn missing_skill_target_is_recorded_as_failed_without_a_run_row() {
        let fx = Fixture::new("ghost-skill");
        let new = NewRoutine {
            name: "calls-ghost".to_owned(),
            cron_expr: "*/1 * * * * *".to_owned(),
            target: RoutineTarget::Skill("does-not-exist".to_owned()),
            backend: None,
            enabled: true,
        };
        let routine = store::add(&fx.conn(), new).unwrap();
        store::roll_forward(
            &fx.conn(),
            routine.id,
            Some(Utc::now() - chrono::Duration::seconds(5)),
        )
        .unwrap();

        let report = tick(&fx.config, &fx.db).await.unwrap();
        assert_eq!(report.fired, 1);
        assert_eq!(report.failed, 1);

        let history = store::list_runs(&fx.conn(), routine.id, 10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, RoutineRunStatus::Failed);
        assert!(history[0].run_id.is_none());
        assert!(
            history[0]
                .detail
                .as_deref()
                .unwrap()
                .contains("does-not-exist")
        );
        // No agent ran, so no `runs` row.
        assert!(runlog::list_runs(&fx.conn(), 10).unwrap().is_empty());
    }

    #[tokio::test]
    async fn reconcile_rolls_past_routine_forward_without_firing() {
        let fx = Fixture::new("reconcile");
        let routine =
            store::add(&fx.conn(), prompt_routine("was-offline", "*/1 * * * * *")).unwrap();
        let missed_slot = Utc::now() - chrono::Duration::minutes(10);
        store::roll_forward(&fx.conn(), routine.id, Some(missed_slot)).unwrap();

        let rolled = reconcile_missed(&fx.config, &fx.db).await.unwrap();
        assert_eq!(rolled, 1);

        let after = store::get(&fx.conn(), routine.id).unwrap().unwrap();
        assert!(after.next_fire_at.unwrap() > Utc::now());
        assert!(
            after.last_fired_at.is_none(),
            "a missed routine did not actually fire"
        );

        let history = store::list_runs(&fx.conn(), routine.id, 10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, RoutineRunStatus::Missed);
        assert!(history[0].run_id.is_none());
        // Nothing executed.
        assert!(runlog::list_runs(&fx.conn(), 10).unwrap().is_empty());
    }

    #[tokio::test]
    async fn spawned_loop_fires_a_due_routine_then_stops_on_shutdown() {
        let fx = Fixture::new("loop");
        let routine = store::add(&fx.conn(), prompt_routine("looped", "*/1 * * * * *")).unwrap();
        store::roll_forward(
            &fx.conn(),
            routine.id,
            Some(Utc::now() - chrono::Duration::seconds(2)),
        )
        .unwrap();

        let handle = spawn_with_interval(
            fx.config.clone(),
            Arc::clone(&fx.db),
            Duration::from_millis(40),
        );

        // Poll until the routine has fired (its next_fire_at moved forward).
        let mut fired = false;
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let current = store::get(&fx.conn(), routine.id).unwrap().unwrap();
            if current.last_fired_at.is_some() {
                fired = true;
                break;
            }
        }
        assert!(fired, "spawned loop should have fired the due routine");

        handle.shutdown().await;

        // After shutdown, no further firings.
        let count_after_stop = store::list_runs(&fx.conn(), routine.id, 100).unwrap().len();
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert_eq!(
            store::list_runs(&fx.conn(), routine.id, 100).unwrap().len(),
            count_after_stop
        );
    }
}
