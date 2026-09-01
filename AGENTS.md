# AGENTS.md

Guidance for OpenCode sessions working in this repository. Full detail lives in
[`CLAUDE.md`](./CLAUDE.md) (commands, test conventions, sub-agent notes) and
[`docs/architecture.md`](./docs/architecture.md) (implemented vs. planned design). Read both
before starting new work. `AGENTS.md` is the compact, high-signal summary.

## What this is

Early-stage Rust + Tauri desktop app: a personal "Agentic OS" / second brain around the
**ARMS framework** (Applications, Routines, Memory, Skills). Milestone **M0 (workspace
scaffold) is done**. Agent backends, skills, memory router, routines, and the real dashboard
UI are **module stubs only** — their doc comments state which milestone implements them.
Do not assume anything in those stubs works.

## Commands

```sh
cargo build --workspace                    # build everything
cargo clippy --workspace -- -D warnings    # lint (MUST be warning-free)
cargo fmt --check                          # format check
cargo test --workspace                     # all tests
cargo test -p axiomata-core                # just the core engine
cargo run -p axiomata-cli                  # headless: init core, print status, exit
cd apps/dashboard && cargo tauri dev       # desktop app (hot-reload)
```

Verify with `clippy -- -D warnings` and `fmt --check` — not just a plain build.

One-time Tauri setup: `cargo install tauri-cli --version "^2" --locked`, then
`cd apps/dashboard && npm install`.

## Architecture (the parts an agent gets wrong)

Cargo workspace (edition 2024); members are `crates/axiomata-core`, `crates/axiomata-macos`,
`crates/axiomata-cli`, and the Tauri shell `apps/dashboard/src-tauri` (package `dashboard`).

- `axiomata-core` is the real engine, with **no Tauri or macOS dependency**. Implemented
  modules: `paths`, `config`, `db`, `error`. `agents`, `skills`, `memory`, `routines` are
  stubs.
- `AxiomataCore::init()` (`crates/axiomata-core/src/lib.rs`) is the **single** entry point,
  called by both `axiomata-cli` and the Tauri `.setup()` hook. Fully idempotent.
- The Tauri `.setup()` hook stores `Mutex<AxiomataCore>` as managed state for future
  `#[tauri::command]` handlers. Only the template `greet` command exists today.

## Data lives in TWO places (easy to get wrong)

1. **`~/.axiomata/`** — app-owned: `config.toml`, `axiomata.db` (SQLite), `logs/`, `skills/`
   (global skills). Overridable via the `AXIOMATA_HOME` env var.
2. **`workspace_root`** — the user's Second-Brain folder (`config.workspace_root`, defaults
   to `~/Axiomata-Workspace`). Holds memory `CLAUDE.md` indexes (M2) and workspace-local
   skills at `<workspace_root>/.claude/skills/` (M1).

When skills land (M1), they merge **both** `skills/` locations, with workspace-local winning
on name collision.

## Conventions

- New shared deps go in root `Cargo.toml` `[workspace.dependencies]`, referenced per-crate as
  `dep.workspace = true`. Never pin versions ad hoc in a crate.
- SQLite via `rusqlite` (bundled, statically linked). DB migrations are append-only SQL in
  `crates/axiomata-core/src/db/migrations/*.sql`, listed in `db::MIGRATIONS` — never edit or
  reorder a released migration.
- DB schema is minimal today (`app_meta` table only); heavier tables are planned for M1–M3.

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