# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project status

Axiomata-OS is an early-stage Rust + Tauri desktop app: a personal "Agentic OS" command
centre / second brain, built around the **ARMS framework** (Applications, Routines,
Memory, Skills — see `ARMS-Agentic-OS-Guide.pdf` for the original inspiration, though the
actual design has since diverged from it in several places).

Milestones **M0 (scaffold)**, **M1 (skills runner)** and **M2 (memory router)** are complete:
the `agents` enum, the single-location skills registry + runner + run log, the `memory`
router (walker / renderer / `sync` / `status` / `notify` watcher), the `list_skills` /
`list_runs` / `run_skill` / `sync_memory` / `get_memory_status` Tauri commands, and a
placeholder UI. Still unimplemented (module stub): the routines scheduler (M3), and the real
module-canvas dashboard UI. The full architecture rationale
and the milestone-by-milestone plan live in
`~/.claude/plans/ich-m-chte-ein-agentic-shimmering-gadget.md` (outside this repo, on the
owner's machine) — read it before starting new work here if it's available; this file
covers what's needed to just build/run/navigate the repo as it stands.

## Commands

```sh
cargo build --workspace                    # build everything
cargo clippy --workspace -- -D warnings    # lint (must be warning-free)
cargo fmt                                  # format
cargo fmt --check                          # verify formatting in CI-style checks
cargo test --workspace                     # run all tests
cargo test -p axiomata-core                # run just the core engine's tests

cargo run -p axiomata-cli                  # headless: init the core, print status, exit
cargo run -p axiomata-cli -- list-skills   # discovered skills (~/.axiomata/skills/)
cargo run -p axiomata-cli -- run-skill <name>   # run a skill, print outcome, exit 1 if it failed
cargo run -p axiomata-cli -- list-runs --limit 20   # recent run history from the DB
cargo run -p axiomata-cli -- memory sync    # regenerate the workspace CLAUDE.md router blocks
cargo run -p axiomata-cli -- memory status  # is the router stale?

cd apps/dashboard && cargo tauri dev       # run the desktop app (hot-reloading dev mode)
```

One-time setup for the Tauri app: `cargo install tauri-cli --version "^2" --locked`, and
`cd apps/dashboard && npm install`.

## Architecture

Cargo workspace (edition 2024), members:
- `crates/axiomata-core` — the actual "OS" engine. No Tauri or macOS dependency, so it can
  in principle run headless elsewhere later. Modules: `paths`, `config`, `db`, `error`,
  `agents`, `skills`, `memory` (all implemented); `routines` (M3) is still a module stub.
- `crates/axiomata-macos` — boundary for future macOS-specific integration (e.g. Mail/
  Calendar access). Untouched stub.
- `crates/axiomata-cli` — thin binary that calls `axiomata_core::AxiomataCore::init()` and
  prints status. Exists specifically so the core can be exercised end-to-end without
  going through the GUI.
- `apps/dashboard/src-tauri` — the Tauri shell (package name `dashboard`), depends on
  `axiomata-core` via a path dependency. Its `.setup()` hook (`src/lib.rs`) calls
  `AxiomataCore::init()` and stores the `AxiomataCore` as managed state for the Tauri
  commands. `AxiomataCore` holds `config` unlocked and only `db` behind a `Mutex`.

New shared dependencies go in root `Cargo.toml` under `[workspace.dependencies]` and are
referenced per-crate as `some_crate.workspace = true` — don't pin versions ad hoc in an
individual crate's `Cargo.toml`.

**Runtime data lives outside the repo**, at `~/.axiomata/` (`config.toml`, `axiomata.db`
SQLite, `logs/`, `skills/` — all skills live here), separate from the user's freely-chosen
Second-Brain workspace folder (`config.workspace_root`, defaults to `~/Axiomata-Workspace`),
which holds only M2 memory content. Override the app-data location with the `AXIOMATA_HOME`
env var (tests do this to avoid touching the real directory).

`AxiomataCore::init()` (`crates/axiomata-core/src/lib.rs`) is the single entry point both
`axiomata-cli` and the Tauri `.setup()` hook call: loads-or-creates `config.toml`, creates
`logs/`, `skills/`, and the workspace root if missing, seeds the bundled `example-skill`
into `~/.axiomata/skills/` if absent (never overwrites), opens the SQLite DB and applies
pending migrations (`crates/axiomata-core/src/db/migrations/*.sql`, listed in
`db::MIGRATIONS`). Fully idempotent — safe to call on every app start.

The `memory` router keeps a generated block between `<!-- AXIOMATA-ROUTER:START/END -->` in
the workspace's `CLAUDE.md` files (root + one per top-level "area"), listing folders and
files with an extracted title. `memory::sync` regenerates them (deterministic — a no-op sync
is byte-identical), `memory::status` reports staleness (a tracked file newer than the root
`CLAUDE.md`, whose mtime *is* the sync marker). `upsert_block` never touches bytes outside
the markers. The Tauri app syncs once at startup and runs a `notify` `MemoryWatcher` (a
reactive stale hint only).

Things worth knowing before touching `skills/` or `agents/`:
- Skills live in **one** place: `~/.axiomata/skills/<name>/SKILL.md`
  (`skills::registry::list_skills` / `find_skill`). They are application-level, not per-vault;
  there is no workspace-local skill location (dropped deliberately — see `docs/architecture.md`
  §4 "Why one skill location"). Frontmatter is parsed with `gray_matter`; the filesystem is
  the only source of truth (skills are never written to the DB). `list_skills` skips a bad
  `SKILL.md`; `find_skill` surfaces its error.
- Agent execution goes through a small enum, not a plugin registry:
  `AgentBackend::ClaudeCode | AgentBackend::Ollama { model }` — deliberately not a
  generic multi-CLI abstraction (see the plan for why). `skills::runner::execute_skill`
  runs a skill without touching the DB (returns an unpersisted `RunRecord`);
  `execute_and_record_skill` = `execute_skill` + `runlog::record_run` (DB row +
  `logs/runs.log` JSONL). The Tauri `run_skill` *command* is the no-DB path plus a
  narrow re-lock to persist.

**Test convention:** any test that mutates the `AXIOMATA_HOME` env var must lock
`crate::test_support::ENV_MUTEX` first (`crates/axiomata-core/src/lib.rs`) — `cargo test`
runs in parallel by default and env vars are process-global. Use
`crate::test_support::unique_temp_dir(prefix)` for scratch directories instead of writing
into a fixed path.

## Sub-agents (use the Rust variants, not the Python-oriented defaults)

The owner's global `~/.claude/CLAUDE.md` defines mandatory automatic sub-agent triggers.
Three of the named agents there (`test-engineer`, `dependency-auditor`,
`performance-analyzer`) are worded for a Python/`uv` stack (pytest, `uv audit`,
SQLAlchemy/Polars) and **do not apply to this repo**. Global, Rust-flavored replacements
exist at `~/.claude/agents/{rust-test-engineer,rust-dependency-auditor,
rust-performance-analyzer}.md` (usable in any Rust project, not just this one) — use
those instead, with the same trigger conditions translated to Rust terms:

- **rust-test-engineer** — invoke after writing or meaningfully modifying any Rust
  function, struct, or module in this workspace; it runs `cargo test` (not `pytest`).
- **rust-dependency-auditor** — invoke whenever any `Cargo.toml` in this workspace is
  modified (not `pyproject.toml`); it runs `cargo audit`/`cargo tree` (not `uv audit`).
- **rust-performance-analyzer** — invoke when new `rusqlite` queries, `tokio` async
  functions, or hot-path data processing are written (not SQLAlchemy/Polars/`async def`).

`architecture-reviewer`, `security-auditor`, `docs-writer`, and `refactoring-specialist`
are already language-agnostic as globally defined and apply here unchanged.
