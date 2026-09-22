# Axiomata-OS

Axiomata-OS is a personal **"Agentic OS"** — a desktop command centre / second brain that
orchestrates AI agents, skills, memory, and scheduled routines on your own machine. It is
inspired by the **ARMS framework** (**A**pplications, **R**outines, **M**emory, **S**kills)
described in [`ARMS-Agentic-OS-Guide.pdf`](./ARMS-Agentic-OS-Guide.pdf) at the root of this
repository, though the actual design has since diverged from that guide in several places —
see [`docs/architecture.md`](./docs/architecture.md) for the current, authoritative design.

The long-term vision: a single always-on desktop app that lets you run reusable "skills"
(headless AI agent tasks), keeps a `CLAUDE.md`-based memory index of a freely-chosen
"Second Brain" workspace folder in sync, and fires scheduled routines against that same
tooling — all visible in one dashboard.

## Project status

Axiomata-OS is early-stage software under active, incremental development.

**Milestone M0 ("workspace scaffold") is complete.** Today the repository is a working
Rust + Tauri desktop app skeleton: on every start it initializes its own app data directory
(`~/.axiomata/`), loads or creates its config file, and opens a migrated SQLite database —
end to end, verified both from a headless CLI and from the Tauri desktop shell.

**Everything past that is not yet implemented:**

- Agent execution (Claude Code / Ollama backends)
- Skills discovery and running
- The memory router that keeps `CLAUDE.md` files in sync with the workspace
- The routines scheduler
- The real dashboard UI (today's window is the unmodified Tauri template)

These exist today only as module stubs with design-intent doc comments. See
[`docs/architecture.md`](./docs/architecture.md) for what's implemented vs. planned, and for
the milestone-by-milestone roadmap (M1–M4).

## Quick start

Requirements: a recent Rust toolchain, Node.js/npm, and (on macOS) Xcode command line tools.
One-time setup for the desktop app:

```sh
cargo install tauri-cli --version "^2" --locked
cd apps/dashboard && npm install
```

Build and check the workspace:

```sh
cargo build --workspace
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

Run the headless core (initializes `~/.axiomata/` and prints status, no GUI):

```sh
cargo run -p axiomata-cli
```

Run the desktop app in hot-reloading dev mode:

```sh
cd apps/dashboard && cargo tauri dev
```

For the full command reference, workspace conventions, and test conventions, see
[`CLAUDE.md`](./CLAUDE.md). For the system architecture — tech stack rationale, crate
layout, data locations, and what's implemented vs. planned — see
[`docs/architecture.md`](./docs/architecture.md).

## Agent status (in the IDE)

Each agent runs in its own **pane** with a terminal (PTY/VT100 stream showing the harness's
live output) and a **status line** across the bottom. The status line displays:

- The harness type, model (if set), and a **branch chip** showing either:
  - The agent's git branch name (with tooltip showing the full worktree path), or
  - **"shared folder"** (if the project is not a git repository, so agents share the project folder)
- A **port chip** showing the reserved port for this agent
- The effective command that will be executed
- A **Restart button** that kills the running PTY session and starts a new one

On the right edge of the pane, a **tab bar** shows four tabs — **Terminal**, **Plan**, **Diffs**, and **Inbox** —
all clickable but only Terminal currently functional. The other three display a placeholder stating when they will arrive:
- **Plan** (M7.2 CP6b): what the agent is working on and where in the plan it stands
- **Diffs** (M7.3): git diffs from the agent's dedicated worktree
- **Inbox** (M7.5): messages from other agents and the user via MCP

Note: A structured **lifecycle status** (working / waiting / done) is planned for CP6 (separate database migration);
today, only the pane's load state ("Preparing this agent's worktree…" or error messages) and the branch/shared-folder chip
reflect the agent's context and readiness.

See [`docs/plans/agentic-ide.md`](./docs/plans/agentic-ide.md) for the full IDE design,
including details on how agents communicate and share work via MCP.

## License

MIT — see [`LICENSE`](./LICENSE).
