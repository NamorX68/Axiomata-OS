//! The Studio's roster: **engines** and **agent roles** (`docs/plans/a2a.md`, A5, CP-A1).
//!
//! * An [`Engine`] is harness + model + environment — the owner's catalog, kept in the app's config.
//! * A [`Role`] is a kind of work with a tier, an engine, limits and instructions — one `AGENT.md` file.
//! * A *session* (a started agent with its worktree) lives in `axiomata-ide` and only refers to both by id.
//!
//! A role never carries a command line, only engine ids, so a role file arriving with a cloned repository can
//! change what an agent is *told*, never which program runs; the owner still confirms such a file by its content
//! hash before it applies (see [`overrides`]).
//!
//! Pure like `axiomata-tasks`: it takes directories and returns data. Nothing here knows Tauri, a database or
//! `axiomata-core`.

mod engine;
mod error;
mod harness;
mod ident;
pub mod overrides;
mod role;
mod store;

pub use engine::{Billing, Engine, MAX_LABEL_CHARS};
pub use error::{Result, RosterError};
pub use harness::Harness;
pub use ident::{MAX_IDENT_LEN, check_slug};
pub use role::{Limits, MAX_INSTRUCTIONS_BYTES, ResolvedLimits, Role, Source, Tier};
pub use store::{
    Loaded, ROLE_FILE, Skipped, default_role, delete_role, grill_role, load_roles, planner_role,
    reviewer_role, save_role, seed_default_roles,
};
