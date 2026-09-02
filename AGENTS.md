# AGENTS.md

Guidance for OpenCode sessions working in this repository. Full detail lives in
[`CLAUDE.md`](./CLAUDE.md) (commands, test conventions, sub-agent notes) and
[`docs/architecture.md`](./docs/architecture.md) (implemented vs. planned design). Read both
before starting new work. `AGENTS.md` is the compact, high-signal summary.

## What this is

Early-stage Rust + Tauri desktop app: a personal "Agentic OS" / second brain around the
**ARMS framework** (Applications, Routines, Memory, Skills). Milestones **M0**, **M1 (skills
runner)** and **M2 (memory router)** are done: `agents` (the `AgentBackend` enum), `skills`
(registry + runner + run log), `memory` (walker / renderer / `sync` / `status`, poll-only
staleness), the `list_skills` / `list_runs` / `run_skill` / `sync_memory` /
`get_memory_status` Tauri commands, and a placeholder UI. The `routines` scheduler (M3) and
the real module-canvas dashboard UI are **module stubs only** — do not assume those work.

## Commands

```sh
cargo build --workspace                    # build everything
cargo clippy --workspace -- -D warnings    # lint (MUST be warning-free)
cargo fmt --check                          # format check
cargo test --workspace                     # all tests
cargo test -p axiomata-core                # just the core engine
cargo run -p axiomata-cli                  # headless: init core, print status, exit
cargo run -p axiomata-cli -- list-skills   # also: run-skill <name>, list-runs, memory sync|status
cd apps/dashboard && cargo tauri dev       # desktop app (hot-reload)
```

Verify with `clippy -- -D warnings` and `fmt --check` — not just a plain build.

One-time Tauri setup: `cargo install tauri-cli --version "^2" --locked`, then
`cd apps/dashboard && npm install`.

## Architecture (the parts an agent gets wrong)

Cargo workspace (edition 2024); members are `crates/axiomata-core`, `crates/axiomata-macos`,
`crates/axiomata-cli`, and the Tauri shell `apps/dashboard/src-tauri` (package `dashboard`).

- `axiomata-core` is the real engine, with **no Tauri or macOS dependency**. Implemented
  modules: `paths`, `config`, `db`, `error`, `agents`, `skills`, `memory`. `routines` (M3)
  is a stub.
- `AxiomataCore::init()` (`crates/axiomata-core/src/lib.rs`) is the **single** entry point,
  called by both `axiomata-cli` and the Tauri `.setup()` hook. Fully idempotent. Also seeds
  the bundled `example-skill` (never overwrites an existing copy).
- `memory`: `memory::sync(config)` regenerates the `<!-- AXIOMATA-ROUTER:START/END -->`
  block in the workspace's `CLAUDE.md` files (deterministic, atomic write, line-wise markers,
  sanitised titles, symlink-refusing, per-file `SyncReport.failed`; refuses a home/`/` root)
  and stamps `~/.axiomata/memory-last-sync.json`; `memory::status(config)` = walk + compare
  against that marker. No file watcher. Startup sync runs on a background thread.
- The Tauri `.setup()` hook stores `AxiomataCore` as managed state — `config` is read
  directly, only `core.db` is behind a `Mutex`. Commands in
  `apps/dashboard/src-tauri/src/commands.rs`: `list_skills`, `list_runs`, `get_run`,
  `run_skill`, `sync_memory`, `get_memory_status`. `run_skill` calls
  `skills::execute_and_record_skill`, which runs the agent
  with no lock held and locks `db` only to write — never hold a lock across `.await`.

## Data lives in TWO places (easy to get wrong)

1. **`~/.axiomata/`** — app-owned: `config.toml`, `axiomata.db` (SQLite), `logs/`, `skills/`
   (**all** skills — application-level, one location). Overridable via `AXIOMATA_HOME`.
2. **`workspace_root`** — the user's Second-Brain folder (`config.workspace_root`, defaults
   to `~/Axiomata-Workspace`). Holds the router `CLAUDE.md` files (M2 writes a generated block into them). No skills here —
   a workspace-local skill location was considered and dropped (untrusted-content path;
   see `docs/architecture.md` §4).

`skills::registry::list_skills()` / `find_skill()` read only `~/.axiomata/skills/`.

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