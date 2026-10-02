//! A Debug Adapter Protocol client, in the layers a debugger needs:
//!
//! * [`frame`] — the wire format (`Content-Length` framing), the only place bytes become JSON;
//! * [`client`] — an adapter process and its requests/responses/events, over threads and channels;
//! * [`session`] — one debugging session: the start-up dance DAP requires (initialize, launch,
//!   breakpoints, configurationDone), typed events, and the queries a UI needs;
//! * [`config`] — `debug.json` and the configurations detected from a project;
//! * [`python`] — which adapter runs Python and how a program is launched under it.
//!
//! Only the DAP methods the Studio speaks are sent; a request the adapter makes of the client
//! (`runInTerminal`, `startDebugging`, …) is answered "not supported" rather than acted on — the
//! adapter is a program from the owner's machine, but what it asks for is still never run for it.

pub mod client;
pub mod config;
pub mod frame;
pub mod native;
pub mod python;
pub mod rust;
pub mod session;

pub use client::{AdapterCommand, Client, DapError, Incoming};
pub use config::{DebugConfig, DebugFile, Language};
pub use python::PythonEnv;
pub use session::{
    Breakpoint, Control, DebugEvent, Frame, Scope, Session, StopState, TerminalHandler,
    TerminalRequest, Variable,
};
