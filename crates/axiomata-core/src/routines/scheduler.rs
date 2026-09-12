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
//! `next_fire_at` is read from the database and never recomputed from the cron
//! expression on load. [`fire_one`] advances a routine past its slot **before**
//! it runs the target, so a crash or kill mid-fire drops that one firing
//! rather than repeating it — firings are **at-most-once**, consistent with the
//! stance that missed fires do not catch up. Before the loop starts,
//! [`serve`] runs one reconciliation pass: any routine already past due (the
//! app was off when it was due) is rolled forward and gets a `Missed` history
//! row — it does **not** fire. So a restart can neither double-fire a routine
//! nor leave one stuck in the past.
//!
//! A *graceful* shutdown ([`SchedulerHandle::shutdown`]) can now also drop a
//! firing mid-flight, not just a crash or kill: `serve_with_interval` races
//! the stop signal against an in-flight [`tick`] itself (not just the wait
//! between ticks), so a stop requested while routines are firing ends the
//! loop right away rather than waiting for the whole pass to finish. That's
//! still safe for the at-most-once guarantee above (the routine already
//! advanced past its slot before its target ran), but it does mean an
//! ordinary app quit can now — same as a crash always could — leave a
//! `routine_runs` history row silently missing for whichever firing was still
//! in flight, rather than that only being possible on a hard kill.

use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;

use crate::config::{Config, ProviderRole};
use crate::error::AxiomataError;
use crate::routines::model::{Routine, RoutineRunStatus, RoutineTarget};
use crate::routines::store::{self, Advance, NewRoutineRun};
use crate::skills::model::{RunRecord, RunSource, RunStatus};
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

/// Runs one poll pass: for every enabled routine whose `next_fire_at` is at or
/// before now, advance it to its next occurrence and then fire it (see
/// [`fire_one`]).
///
/// Each due routine fires at most once per pass even if several of its
/// scheduled slots have elapsed — `next_fire_at` jumps to the next occurrence
/// strictly after now, never replaying the backlog.
///
/// Due routines fire **concurrently** (one [`tokio::task::JoinSet`] task
/// each), not sequentially — with several routines due in the same pass, a
/// slow agent no longer holds up the others' firing behind it for up to its
/// own timeout. No separate cap is applied here: the real resource to bound
/// is concurrently-running `opencode` child processes, which
/// [`crate::agents::agent_slots`] already caps process-wide across every
/// caller (routines, manual "run now", chat alike) — an Ollama-backed prompt
/// routine is the one target this pass doesn't gate, since it is an HTTP call
/// rather than a spawned process.
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

    let mut in_flight = tokio::task::JoinSet::new();
    for routine in due {
        let config = config.clone();
        let db = Arc::clone(db);
        in_flight.spawn(async move {
            let outcome = fire_one(&routine, &config, &db, now).await;
            (routine.name, outcome)
        });
    }

    let mut report = TickReport::default();
    while let Some(joined) = in_flight.join_next().await {
        match joined {
            Ok((_, Ok(RoutineRunStatus::Success))) => {
                report.fired += 1;
                report.succeeded += 1;
            }
            Ok((_, Ok(_))) => {
                report.fired += 1;
                report.failed += 1;
            }
            Ok((name, Err(err))) => report.errors.push(format!("{name}: {err}")),
            // A `fire_one` task itself panicked — treat like any other
            // per-routine failure rather than propagating, consistent with
            // "one bad routine cannot stop the others" above.
            Err(join_err) => report
                .errors
                .push(format!("routine task panicked: {join_err}")),
        }
    }
    Ok(report)
}

/// Fires one routine: advance it past its slot first (at-most-once), then run
/// the target, then record the outcome in `runs` (when an agent ran) and
/// `routine_runs`.
///
/// A routine whose stored cron expression no longer parses is **disabled** and
/// given a `Failed` history row, rather than being retried every tick forever.
/// Returns `Err` only on a database write failure before the agent runs — the
/// routine stays due and the next tick retries it, with nothing executed yet.
async fn fire_one(
    routine: &Routine,
    config: &Config,
    db: &Arc<Mutex<Connection>>,
    fired_at: DateTime<Utc>,
) -> Result<RoutineRunStatus, AxiomataError> {
    let scheduled_for = routine.next_fire_at.unwrap_or(fired_at);

    // Advance out of the due set BEFORE executing, so a crash mid-fire loses
    // this firing rather than repeating it. A cron that no longer parses can't
    // be scheduled at all — disable the routine and record why.
    let advanced = {
        let conn = lock(db);
        store::advance(&conn, routine.id, Advance::Fired(fired_at))
    };
    if let Err(err) = advanced {
        return match err {
            AxiomataError::CorruptRoutineRow { reason, .. } => {
                let conn = lock(db);
                store::set_enabled(&conn, routine.id, false)?;
                store::record_run(
                    &conn,
                    routine.id,
                    NewRoutineRun {
                        run_id: None,
                        scheduled_for,
                        fired_at,
                        status: RoutineRunStatus::Failed,
                        detail: Some(format!("routine disabled — {reason}")),
                    },
                )?;
                Ok(RoutineRunStatus::Failed)
            }
            other => Err(other),
        };
    }

    // Spend guardrail: a routine firing through a paid provider counts against
    // the same daily cap as a manual run. Over the cap, skip the agent call
    // entirely and record the firing as failed (provider-hardening
    // checkpoint 5). No-op on the Anthropic subscription path.
    let cap_check = {
        let conn = lock(db);
        crate::spend::guard_redirected_turn(&conn, config, ProviderRole::Skill)
    };
    let outcome = match cap_check {
        // The agent call: no lock held (it awaits, possibly for the whole timeout).
        Ok(()) => execute_target(routine, config).await,
        Err(capped) => Err(capped),
    };

    let conn = lock(db);
    let (status, detail, run_id) = match outcome {
        Ok(mut record) => {
            let status = if record.status == RunStatus::Success {
                RoutineRunStatus::Success
            } else {
                RoutineRunStatus::Failed
            };
            let detail = record.error.clone();
            // `execute_target` (and everything it calls) has no notion of
            // "fired by a routine" — it's the same runner path a manual run
            // uses. Stamp it here, the one place that does know.
            record.source = RunSource::Routine;
            // Move the record in — it can carry up to ~2 MiB of captured output.
            let stored = runlog::record_run(&conn, record)?;
            (status, detail, stored.id)
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
    Ok(status)
}

/// Dispatches to the skill runner or the raw-prompt runner.
///
/// `Err` here means the target could not be executed at all (a skill routine
/// whose skill no longer exists); every other outcome, including a non-zero
/// exit or an unresolvable prompt backend, is an `Ok(RunRecord)`.
async fn execute_target(routine: &Routine, config: &Config) -> Result<RunRecord, AxiomataError> {
    match &routine.target {
        RoutineTarget::Skill(name) => runner::execute_skill(name, config).await,
        RoutineTarget::Prompt(text) => {
            runner::execute_prompt(
                &routine.name,
                text.clone(),
                routine.backend.as_deref().unwrap_or(DEFAULT_PROMPT_BACKEND),
                config,
            )
            .await
        }
    }
}

/// Rolls every already-past routine forward without firing it, recording a
/// `Missed` history row for each. Run once at startup, before the loop.
///
/// Returns how many routines were rolled forward. A per-routine failure is
/// logged and skipped — it does not abort the sweep (the next tick will pick
/// that routine up and, if its cron is broken, disable it).
async fn reconcile_missed(db: &Arc<Mutex<Connection>>) -> Result<usize, AxiomataError> {
    let now = Utc::now();
    let stale = {
        let conn = lock(db);
        store::due_routines(&conn, now)?
    };

    let mut rolled = 0;
    for routine in stale {
        match reconcile_one(db, &routine, now) {
            Ok(()) => rolled += 1,
            Err(err) => {
                tracing::warn!(routine = %routine.name, %err, "could not reconcile a missed routine");
            }
        }
    }
    Ok(rolled)
}

/// Writes one `Missed` row and rolls one routine forward, under a single lock.
fn reconcile_one(
    db: &Arc<Mutex<Connection>>,
    routine: &Routine,
    now: DateTime<Utc>,
) -> Result<(), AxiomataError> {
    let scheduled_for = routine.next_fire_at.unwrap_or(now);
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
    store::advance(&conn, routine.id, Advance::Skipped)?;
    Ok(())
}

/// Stops a running scheduler loop. Sending the signal — explicitly via
/// [`SchedulerHandle::shutdown`], or implicitly on `Drop` — makes the loop
/// break after its current tick.
///
/// Holds only the stop signal, never the loop's [`tokio::task::JoinHandle`] —
/// a caller that needs to *wait* for the loop to actually finish (rather than
/// just requesting the stop) keeps the `JoinHandle` [`spawn`] returns
/// alongside this handle and awaits it directly.
pub struct SchedulerHandle {
    stop: tokio::sync::watch::Sender<bool>,
}

impl SchedulerHandle {
    /// Creates a handle with no loop attached yet — call [`subscribe`](Self::subscribe)
    /// for a receiver to hand to [`serve`] on whatever runtime you have (the
    /// Tauri shell does this: its `.setup()` is not itself inside a Tokio
    /// runtime, so it spawns `serve` via `tauri::async_runtime` separately).
    pub fn new() -> Self {
        let (stop, _unused_rx) = tokio::sync::watch::channel(false);
        Self { stop }
    }

    /// Vends a receiver watching this handle's stop signal. A `watch`
    /// channel supports any number of readers, so this may be called more
    /// than once — though today only one loop per handle ever subscribes.
    ///
    /// Must be called *before* [`shutdown`](Self::shutdown) for the returned
    /// receiver to ever see it: a `watch::Receiver`'s `changed()` only fires
    /// on a transition *after* it starts watching, not on the value it was
    /// already at when created. Every real caller here satisfies this by
    /// construction (`subscribe` always runs before the loop it feeds is
    /// even spawned), but it's a sequencing contract worth stating rather
    /// than leaving implicit.
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<bool> {
        self.stop.subscribe()
    }

    /// Signals every subscribed loop to stop after its current tick. Does
    /// not itself wait for that to happen — see the struct docs. Takes `&self`
    /// (sending on the underlying `watch` channel never needs ownership), so
    /// this can be called through a shared reference — e.g. Tauri's managed
    /// `State<'_, SchedulerHandle>` — without taking the handle out first.
    pub fn shutdown(&self) {
        let _ = self.stop.send(true);
    }
}

impl Default for SchedulerHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SchedulerHandle {
    fn drop(&mut self) {
        // Best-effort: tell the loop to stop, the same signal `shutdown`
        // sends. The task ends on its own at the next tick boundary (or when
        // the process exits, whichever comes first).
        let _ = self.stop.send(true);
    }
}

/// Starts the routine scheduler on the current Tokio runtime.
///
/// Returns the [`SchedulerHandle`] to request a stop with, and the loop's own
/// [`tokio::task::JoinHandle`] for a caller that wants to await its actual
/// completion after requesting one (rather than just firing the request and
/// moving on).
///
/// # Panics
///
/// Panics if called outside a Tokio runtime. The Tauri shell, whose `.setup()`
/// has no runtime, uses [`SchedulerHandle::new`] + [`SchedulerHandle::subscribe`]
/// + [`serve`] instead.
pub fn spawn(
    config: Arc<RwLock<Config>>,
    db: Arc<Mutex<Connection>>,
) -> (SchedulerHandle, tokio::task::JoinHandle<()>) {
    spawn_with_interval(config, db, POLL_INTERVAL)
}

/// [`spawn`] with a caller-chosen poll interval — used by tests to drive the
/// loop without waiting [`POLL_INTERVAL`].
pub(crate) fn spawn_with_interval(
    config: Arc<RwLock<Config>>,
    db: Arc<Mutex<Connection>>,
    interval: Duration,
) -> (SchedulerHandle, tokio::task::JoinHandle<()>) {
    let handle = SchedulerHandle::new();
    let stop_rx = handle.subscribe();
    let join = tokio::spawn(serve_with_interval(config, db, stop_rx, interval));
    (handle, join)
}

/// Runs the scheduler loop until the stop signal flips to `true` or its sender
/// drops. This is the whole loop as a future; run it with `tokio::spawn`,
/// `tauri::async_runtime::spawn`, or by awaiting it directly.
///
/// `config` is the same shared, lockable config the rest of the app reads —
/// not an owned snapshot — so a live Settings-dialog change (provider/model)
/// reaches the *next* tick without an app restart. Each tick reads a fresh
/// clone right before running (see the loop body below); the lock is never
/// held across an `.await`.
pub async fn serve(
    config: Arc<RwLock<Config>>,
    db: Arc<Mutex<Connection>>,
    stop_rx: tokio::sync::watch::Receiver<bool>,
) {
    serve_with_interval(config, db, stop_rx, POLL_INTERVAL).await
}

/// [`serve`] with an explicit poll interval.
async fn serve_with_interval(
    config: Arc<RwLock<Config>>,
    db: Arc<Mutex<Connection>>,
    mut stop_rx: tokio::sync::watch::Receiver<bool>,
    interval: Duration,
) {
    match reconcile_missed(&db).await {
        Ok(0) => {}
        Ok(n) => tracing::info!(rolled_forward = n, "routine scheduler skipped missed fires"),
        Err(err) => tracing::warn!(%err, "routine scheduler startup reconciliation failed"),
    }

    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                // Race the tick itself against the stop signal too — not
                // just the wait *between* ticks — so a stop requested while
                // several due routines are firing concurrently ends the loop
                // right away instead of waiting for the whole pass to finish.
                // Dropping the `tick` future here aborts any of its
                // `fire_one` tasks still in flight; that is no worse than the
                // process being killed outright, which the at-most-once
                // firing design already tolerates (see the module doc).
                //
                // Read + clone a fresh snapshot right here, synchronously —
                // the guard is a temporary, dropped at the end of this `let`
                // statement, well before the `.await` below. This is what
                // lets a live Settings-dialog provider/model change reach the
                // very next tick without a restart.
                let config_snapshot = config
                    .read()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .clone();
                tokio::select! {
                    outcome = tick(&config_snapshot, &db) => match outcome {
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
                        if result.is_err() || *stop_rx.borrow() {
                            break;
                        }
                    }
                }
            }
            result = stop_rx.changed() => {
                // Sender dropped or told us to stop.
                if result.is_err() || *stop_rx.borrow() {
                    break;
                }
            }
        }
    }
}

/// Locks the shared connection, recovering the guard if a previous panic
/// poisoned it rather than propagating — consistent with
/// [`AxiomataCore::db_lock`](crate::AxiomataCore::db_lock) and the rest of
/// the crate.
fn lock(db: &Arc<Mutex<Connection>>) -> std::sync::MutexGuard<'_, Connection> {
    db.lock().unwrap_or_else(|poison| poison.into_inner())
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
        store::set_next_fire_at(&fx.conn(), routine.id, Some(past)).unwrap();

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

        // A run row exists (the Ollama failure is still a recorded run),
        // attributed to the routine rather than a manual trigger.
        let runs = runlog::list_runs(&fx.conn(), 10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].source, RunSource::Routine);

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
        store::set_next_fire_at(&fx.conn(), routine.id, Some(long_ago)).unwrap();

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
        store::set_next_fire_at(
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
        store::set_next_fire_at(
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
    async fn a_routine_with_an_unparseable_stored_cron_is_disabled_not_looped() {
        let fx = Fixture::new("corrupt-cron");
        let routine = store::add(&fx.conn(), prompt_routine("broken", "*/1 * * * * *")).unwrap();
        // Corrupt the stored expression the way only a hand-edit or a future
        // parser change could, then make it due.
        fx.conn()
            .execute(
                "UPDATE routines SET cron_expr = 'not a cron' WHERE id = ?1",
                [routine.id],
            )
            .unwrap();
        store::set_next_fire_at(
            &fx.conn(),
            routine.id,
            Some(Utc::now() - chrono::Duration::seconds(5)),
        )
        .unwrap();

        let report = tick(&fx.config, &fx.db).await.unwrap();
        assert_eq!(report.failed, 1);
        assert!(report.errors.is_empty());

        let after = store::get(&fx.conn(), routine.id).unwrap().unwrap();
        assert!(
            !after.enabled,
            "a routine that can't be scheduled is disabled"
        );

        let history = store::list_runs(&fx.conn(), routine.id, 10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, RoutineRunStatus::Failed);
        assert!(history[0].detail.as_deref().unwrap().contains("disabled"));
        // Nothing executed.
        assert!(runlog::list_runs(&fx.conn(), 10).unwrap().is_empty());

        // It stays put on the next tick.
        assert_eq!(tick(&fx.config, &fx.db).await.unwrap().fired, 0);
    }

    #[tokio::test]
    async fn reconcile_rolls_past_routine_forward_without_firing() {
        let fx = Fixture::new("reconcile");
        let routine =
            store::add(&fx.conn(), prompt_routine("was-offline", "*/1 * * * * *")).unwrap();
        let missed_slot = Utc::now() - chrono::Duration::minutes(10);
        store::set_next_fire_at(&fx.conn(), routine.id, Some(missed_slot)).unwrap();

        let rolled = reconcile_missed(&fx.db).await.unwrap();
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
    async fn subscribe_may_be_called_more_than_once_with_each_reader_seeing_the_stop() {
        // Exercises `SchedulerHandle::subscribe`'s doc claim directly: a
        // `watch` channel supports any number of independent readers, so two
        // subscribers taken before a single `shutdown()` must each observe
        // it on their own, without one reader's `changed()` consuming the
        // signal for the other.
        let handle = SchedulerHandle::new();
        let mut first = handle.subscribe();
        let mut second = handle.subscribe();
        assert!(!*first.borrow());
        assert!(!*second.borrow());

        handle.shutdown();

        first.changed().await.unwrap();
        assert!(*first.borrow());
        second.changed().await.unwrap();
        assert!(*second.borrow());
    }

    #[tokio::test]
    async fn spawned_loop_fires_a_due_routine_then_stops_on_shutdown() {
        let fx = Fixture::new("loop");
        let routine = store::add(&fx.conn(), prompt_routine("looped", "*/1 * * * * *")).unwrap();
        store::set_next_fire_at(
            &fx.conn(),
            routine.id,
            Some(Utc::now() - chrono::Duration::seconds(2)),
        )
        .unwrap();

        let (handle, join) = spawn_with_interval(
            Arc::new(RwLock::new(fx.config.clone())),
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

        // `shutdown` only requests the stop; awaiting the loop's own
        // `JoinHandle` is what proves it actually ended, deterministically
        // rather than via a fixed sleep-and-hope.
        handle.shutdown();
        join.await.unwrap();

        // After shutdown, no further firings.
        let count_after_stop = store::list_runs(&fx.conn(), routine.id, 100).unwrap().len();
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert_eq!(
            store::list_runs(&fx.conn(), routine.id, 100).unwrap().len(),
            count_after_stop
        );
    }

    #[tokio::test]
    async fn spawned_loop_reads_a_config_change_made_after_it_starts_running() {
        // Proves the loop holds the *same* shared `Arc<RwLock<Config>>` as the
        // rest of the app, not a value snapshot frozen at `spawn_with_interval`
        // time — a live Settings-dialog edit must reach the very next tick.
        //
        // The observable is deliberately something that fails at OS-level
        // process spawn, *before* `execve` ever runs: an `opencode` routine
        // whose `cwd` (`config.workspace_root`) is invalid. That happens
        // inside the child, pre-exec, so the real `opencode` binary on this
        // machine's PATH is never actually invoked — no live agent call, no
        // network. Two different *kinds* of invalid path yield two
        // distinguishable `io::Error`s (`ENOENT` vs. `ENOTDIR`), so the
        // recorded failure detail proves which of the two config values was
        // active when the routine actually fired.
        let fx = Fixture::new("live-config");
        let missing_dir = fx.home.join("workspace-does-not-exist");
        let not_a_dir = fx.home.join("workspace-is-a-file");
        fs::write(&not_a_dir, b"not a directory").unwrap();

        let mut config = fx.config.clone();
        config.workspace_root = missing_dir;
        let shared_config = Arc::new(RwLock::new(config));

        let routine = store::add(
            &fx.conn(),
            NewRoutine {
                name: "live-config-probe".to_owned(),
                cron_expr: "*/1 * * * * *".to_owned(),
                target: RoutineTarget::Prompt("irrelevant — spawn fails first".to_owned()),
                backend: Some("opencode".to_owned()),
                enabled: true,
            },
        )
        .unwrap();
        // Not due yet: held back until well after the config swap below, so
        // the loop's first live firing observes the *new* value rather than
        // the one present when the loop started.
        store::set_next_fire_at(
            &fx.conn(),
            routine.id,
            Some(Utc::now() + chrono::Duration::milliseconds(400)),
        )
        .unwrap();

        let (handle, join) = spawn_with_interval(
            Arc::clone(&shared_config),
            Arc::clone(&fx.db),
            Duration::from_millis(30),
        );

        // Several poll intervals elapse here against the *old* config value
        // with nothing due yet — nothing should fire during this window.
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(
            store::list_runs(&fx.conn(), routine.id, 10)
                .unwrap()
                .is_empty(),
            "routine was not due yet and must not have fired early"
        );
        shared_config.write().unwrap().workspace_root = not_a_dir;

        let mut history = Vec::new();
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            history = store::list_runs(&fx.conn(), routine.id, 10).unwrap();
            if !history.is_empty() {
                break;
            }
        }

        handle.shutdown();
        join.await.unwrap();

        assert_eq!(history.len(), 1, "routine should have fired exactly once");
        let detail = history[0].detail.as_deref().unwrap_or_default();
        assert!(
            detail.contains("Not a directory") || detail.contains("os error 20"),
            "expected the failure to reflect the *post-change* workspace_root \
             (a file, not a directory — ENOTDIR), proving the running loop \
             re-read the shared config rather than the value captured at \
             spawn time: {detail}"
        );
    }
}
