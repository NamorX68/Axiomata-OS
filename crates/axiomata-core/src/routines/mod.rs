//! Scheduled routines: a cron expression bound to a skill or a raw prompt,
//! fired unattended by a background poll loop.
//!
//! Layout mirrors [`crate::skills`]:
//! - [`model`] — the [`Routine`] / [`RoutineRun`] data types and the derived
//!   [`RoutineState`].
//! - [`schedule`] — cron parsing and next-fire computation (wraps the `cron`
//!   crate; 6-7 field format, seconds first).
//! - [`store`] — all SQL for the `routines` and `routine_runs` tables.
//! - [`scheduler`] — the single Tokio task that polls `store` roughly every
//!   30 seconds and fires whatever is due.
//!
//! `next_fire_at` is persisted and authoritative: it survives restarts and is
//! never recomputed from the cron expression on load, so restarting can
//! neither double-fire nor drop a routine. A fire time that passed while the
//! app was down is rolled forward without firing.
//!
//! Implemented in M3.

pub mod model;
pub mod schedule;
pub mod scheduler;
pub mod store;

// Curated facade so the CLI and Tauri commands don't bind to the internal
// module layout.
pub use model::{NewRoutine, Routine, RoutineRun, RoutineRunStatus, RoutineState, RoutineTarget};
pub use scheduler::{SchedulerHandle, serve, spawn, tick};
