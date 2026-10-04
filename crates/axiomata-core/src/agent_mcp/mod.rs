//! The MCP server agents use to reach the mailbox and the board (`docs/plans/a2a.md` CP-A4, decisions A1, A28, A39).
//!
//! `axiomata-cli mcp-serve` is one server per agent session: started by the session's harness, speaking MCP over
//! stdio, working directly on the database like the CLI does (SQLite in WAL mode with a busy timeout serves several
//! processes). A session's identity, card and rights come from the environment the studio started it with, see
//! [`Context`]. While the server runs it holds the session's presence lock ([`axiomata_ide::presence`]) — that is how
//! the mailbox knows the session is alive.
//!
//! Layers: [`protocol`] frames JSON-RPC lines, [`tools`] implements the tools over the core functions, [`context`]
//! says who is speaking and what it may do.

pub mod context;
pub mod protocol;
pub mod tools;

#[cfg(test)]
mod tests;

use std::io::{self, BufRead, Write};

pub use context::{Capabilities, Context, ContextError, Creates};
pub use protocol::{MAX_LINE_BYTES, handle_line, run};

/// Serves one session on stdin and stdout until the client closes them, holding the session's presence lock for as long
/// as it runs.
///
/// # Errors
///
/// An I/O error on the streams, or on the presence lock for a reason other than another server already holding it.
pub fn serve_stdio(ctx: &Context) -> io::Result<()> {
    // Kept in a binding, not dropped: releasing it is what tells the others this session is gone.
    let _presence =
        axiomata_ide::presence::hold(&ctx.roots, ctx.agent.id).map_err(io::Error::other)?;
    if _presence.is_none() {
        // Another server holds the lock — maybe the one the harness is replacing. Wait for it on a thread of its own
        // and take over when it ends, so the session never counts as gone while this server runs.
        let roots = ctx.roots.clone();
        let id = ctx.agent.id;
        std::thread::spawn(move || {
            if let Ok(presence) = axiomata_ide::presence::hold_blocking(&roots, id) {
                // Owned for the rest of the process's life; the thread exists only to keep the lock.
                loop {
                    std::thread::park();
                    let _ = &presence;
                }
            }
        });
    }
    let stdin = io::stdin();
    let stdout = io::stdout();
    run(ctx, stdin.lock(), stdout.lock())
}

/// Runs the server over any pair of streams (the in-process test client uses this).
///
/// # Errors
///
/// As [`protocol::run`].
pub fn serve<R: BufRead, W: Write>(ctx: &Context, reader: R, writer: W) -> io::Result<()> {
    run(ctx, reader, writer)
}
