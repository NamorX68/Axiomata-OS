//! Client for Opencode 2's local background service.
//!
//! Opencode 2 runs one shared background server per user; every client — the
//! terminal UI, `opencode run`, and Axiomata — talks to it over HTTP. This
//! crate is Axiomata's client (`docs/plans/opencode2.md`, decisions Q1/Q2/Q10):
//!
//! - [`Service`] finds the running service, logs in, checks its version and
//!   starts it when it is not running ([`service`]).
//! - [`EventStream`] reads the server's event stream ([`events`]).
//! - Sessions are created, prompted, read and steered through [`Service`]'s
//!   methods ([`session`]).
//! - [`Service::run_turn`] puts those together into one agent turn: a prompt,
//!   its execution, and what it produced ([`turn`]).
//!
//! The documented surface is <https://opencode.ai/v2/docs>. The one piece that
//! is not documented is how a local client logs in; it lives in
//! [`service`] alone (see there and
//! <https://github.com/anomalyco/opencode/issues/51724>).
//!
//! No dependency on `axiomata-core`, like `axiomata-terminal` and
//! `axiomata-files`: the skill runner (in core) and the agentic IDE
//! (`axiomata-ide`) both use it.

pub mod error;
pub mod events;
pub mod service;
pub mod session;
pub mod turn;

pub use error::OpencodeError;
pub use events::{Event, EventStream};
pub use service::Service;
pub use session::{ModelRef, NewSession, PermissionRule};
pub use turn::{TurnOutcome, TurnRequest, TurnSession, unattended_permissions};
