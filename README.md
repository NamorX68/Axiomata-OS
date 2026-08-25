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

## License

MIT — see [`LICENSE`](./LICENSE).
