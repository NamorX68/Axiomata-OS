//! Standalone PTY + terminal-emulation engine.
//!
//! No dependency on Tauri or `axiomata-core` — this crate only knows about
//! PTYs, raw bytes, and (from a later checkpoint on) its own screen model,
//! so it stays unit-testable and runnable on its own (see the `term-poc`
//! binary). See `docs/plans/terminal.md` for the full phased plan.
//!
//! Checkpoint 0 (this state): spawn a shell in a PTY and pass bytes through
//! unmodified — there is no ANSI/VT100 interpretation yet.

mod pty;

pub use pty::PtySession;
