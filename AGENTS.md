# AGENTS.md

Guidance for OpenCode sessions working in this repository. Full detail lives in
[`CLAUDE.md`](./CLAUDE.md) (commands, test conventions, sub-agent notes) and
[`docs/architecture.md`](./docs/architecture.md) (implemented vs. planned design). Read both
before starting new work. `AGENTS.md` is the compact, high-signal summary.

## What this is

Early-stage Rust + Tauri desktop app: a personal "Agentic OS" / second brain around the
**ARMS framework** (Applications, Routines, Memory, Skills). Milestones **M0 (workspace
scaffold)** and **M1 (skills runner)** are done: `agents` (the `AgentBackend` enum),
`skills` (registry + runner + run log), the `list_skills` / `list_runs` / `run_skill` Tauri
commands, and a placeholder skills UI. The `memory` router (M2), `routines` scheduler (M3),
and the real module-canvas dashboard UI are **module stubs only** — their doc comments state
which milestone implements them. Do not assume anything in those stubs works.

## Commands

```sh
cargo build --workspace                    # build everything
cargo clippy --workspace -- -D warnings    # lint (MUST be warning-free)
cargo fmt --check                          # format check
cargo test --workspace                     # all tests
cargo test -p axiomata-core                # just the core engine
cargo run -p axiomata-cli                  # headless: init core, print status, exit
cargo run -p axiomata-cli -- list-skills   # discovered skills; also: run-skill <name>, list-runs
cd apps/dashboard && cargo tauri dev       # desktop app (hot-reload)
```

Verify with `clippy -- -D warnings` and `fmt --check` — not just a plain build.

One-time Tauri setup: `cargo install tauri-cli --version "^2" --locked`, then
`cd apps/dashboard && npm install`.

## Architecture (the parts an agent gets wrong)

Cargo workspace (edition 2024); members are `crates/axiomata-core`, `crates/axiomata-macos`,
`crates/axiomata-cli`, and the Tauri shell `apps/dashboard/src-tauri` (package `dashboard`).

- `axiomata-core` is the real engine, with **no Tauri or macOS dependency**. Implemented
  modules: `paths`, `config`, `db`, `error`, `agents`, `skills`. `memory` (M2) and
  `routines` (M3) are stubs.
- `AxiomataCore::init()` (`crates/axiomata-core/src/lib.rs`) is the **single** entry point,
  called by both `axiomata-cli` and the Tauri `.setup()` hook. Fully idempotent. Also seeds
  the bundled `example-skill` (never overwrites an existing copy).
- The Tauri `.setup()` hook stores `Mutex<AxiomataCore>` as managed state. Commands live in
  `apps/dashboard/src-tauri/src/commands.rs`: `list_skills`, `list_runs`, `run_skill`.
  `run_skill` clones the config, drops the lock, awaits the agent, re-locks only to persist
  — never hold a `MutexGuard` across `.await`.

## Data lives in TWO places (easy to get wrong)

1. **`~/.axiomata/`** — app-owned: `config.toml`, `axiomata.db` (SQLite), `logs/`, `skills/`
   (global skills). Overridable via the `AXIOMATA_HOME` env var.
2. **`workspace_root`** — the user's Second-Brain folder (`config.workspace_root`, defaults
   to `~/Axiomata-Workspace`). Holds memory `CLAUDE.md` indexes (M2) and workspace-local
   skills at `<workspace_root>/.claude/skills/`.

`skills::registry::list_skills` merges **both** `skills/` locations, with workspace-local
winning on name collision.

## Conventions

- New shared deps go in root `Cargo.toml` `[workspace.dependencies]`, referenced per-crate as
  `dep.workspace = true`. Never pin versions ad hoc in a crate.
- SQLite via `rusqlite` (bundled, statically linked). DB migrations are append-only SQL in
  `crates/axiomata-core/src/db/migrations/*.sql`, listed in `db::MIGRATIONS` — never edit or
  reorder a released migration.
- DB tables: `app_meta` (0001), `runs` (0002 — skill run history, mirrored to
  `logs/runs.log`). `routines` / `routine_runs` are planned for M3.

## Test conventions

`cargo test` runs in parallel; env vars are process-global. Any test that mutates
`AXIOMATA_HOME` must first lock `crate::test_support::ENV_MUTEX`. Use
`crate::test_support::unique_temp_dir(prefix)` for scratch dirs — never a fixed path.

## Docs to trust and not trust

- Trust `docs/architecture.md` for what's implemented vs. planned (it's explicit about both).
- The `ARMS-Agentic-OS-Guide.pdf` is **inspiration only**; the design has diverged from it.
- `README.md` and `CLAUDE.md` reflect current state. The full milestone plan lives outside
  the repo at `~/.claude/plans/ich-m-chte-ein-agentic-shimmering-gadget.md` on the owner's
  machine; read it if present before starting milestone work.