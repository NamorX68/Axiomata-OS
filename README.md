# Axiomata-OS

Axiomata-OS is a personal **"Agentic OS"** — a desktop command centre / second brain that
orchestrates AI agents, skills, memory, and scheduled routines on your own machine. It is
inspired by the **ARMS framework** (**A**pplications, **R**outines, **M**emory, **S**kills)
described in [`ARMS-Agentic-OS-Guide.pdf`](./ARMS-Agentic-OS-Guide.pdf) at the root of this
repository, though the actual design has since diverged from that guide in several places —
see [`docs/architecture.md`](./docs/architecture.md) for the current, authoritative design.

One desktop app that runs reusable "skills" (headless AI agent tasks), keeps a
`CLAUDE.md`-based memory index of a freely-chosen "Second Brain" workspace folder in sync,
fires scheduled routines against the same tooling, and hosts its own IDE and editor for
working alongside coding agents — all in one dashboard.

## Project status

Axiomata-OS is a working personal tool under active, incremental development (Rust + Tauri,
Svelte front end, macOS first). What exists today:

- **Skills and routines** — headless agent tasks (`~/.axiomata/skills/`) run on the Opencode
  CLI against a per-role model provider (Anthropic, OpenRouter, Ollama), with a daily cost cap;
  a cron scheduler with full CRUD.
- **Memory router** — keeps the `CLAUDE.md` router blocks of the Second-Brain workspace in sync.
- **Dashboard** — a module canvas (Calendar, Reminders, Mail, ToDo, Kanban board, Terminal, …),
  themes, and the particle-graph Second Brain.
- **Agentic IDE** (M7) — projects with a dock layout, agent harnesses (Claude Code, Opencode)
  in PTY panes, one git worktree per agent with a live status and plan, and a git layer for
  diffs, commits and take-over.
- **Own editor** (ED0–ED5) — a file service guarded per root, a TypeScript editor engine on a
  rope, tree-sitter highlighting, Vi mode, multiple cursors, find bar and project search,
  folding, sticky scroll, minimap, installed Mac fonts, file tree with tabs and quick open.

Next: language servers in the editor (ED6). M4 (always-on background scheduling) was dropped.
See [`docs/architecture.md`](./docs/architecture.md) for the full design and milestone history,
and `docs/plans/` for the detailed plans.

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
