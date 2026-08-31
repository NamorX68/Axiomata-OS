# Axiomata-OS Architecture

This document describes the architecture of Axiomata-OS as it actually exists in this
repository today, and — clearly separated — the design that later milestones will build on
top of it. If you have never seen this repo before, this is the place to start after the
[README](../README.md).

## 1. Vision

Axiomata-OS is a personal "Agentic OS": a single desktop application that acts as
a command centre / second brain for one user. (The long-term goal is an always-on
application; the MVP milestones below run it as an ordinary app the user starts and quits —
see §7, M4.) It is organized around the **ARMS framework**
(**A**pplications, **R**outines, **M**emory, **S**kills), the model described in
[`ARMS-Agentic-OS-Guide.pdf`](../ARMS-Agentic-OS-Guide.pdf) at the repository root. The core
idea: instead of scattering AI-agent usage across ad-hoc scripts and chat sessions, give it a
proper home with persistent state — reusable **skills** (packaged, reusable agent tasks),
a **memory** layer that keeps a personal knowledge workspace machine-readable for an agent,
**routines** that fire skills on a schedule without a human present, and, eventually,
**applications** — deeper integrations with things like mail and calendar.

The actual implementation has diverged from the PDF guide in a number of concrete design
decisions (documented below); the PDF should be read as inspiration, not as a specification
for this codebase.

## 2. Tech stack and why

- **Rust** for the core engine (`axiomata-core`). The scheduler, skill runner, and memory
  router are exactly the kind of long-running, I/O-heavy, concurrency-sensitive logic Rust
  is well suited for, and the core has no GUI dependency, so it can in principle run headless
  on another machine later (see the ARMS "always-on machine in the cloud" idea).
- **Tauri** for the desktop shell, chosen deliberately over both **Electron** and a native
  **SwiftUI** app:
  - Electron was rejected primarily for its resource footprint (bundling a full Chromium
    per app instance) for what is meant to be an always-running background application.
  - Tauri uses the OS's native WebView instead of a bundled browser engine, which keeps the
    resource footprint far closer to a native app while still rendering the dashboard with
    ordinary web technology (HTML/CSS/Canvas, later D3.js or three.js). That matters because
    the planned dashboard UI is a free-form canvas of user-arranged widget modules with a
    particle-graph visualization at its centre (see `Screenshots/` for the target look) —
    something substantially harder to build in SwiftUI than with web-native graphics
    libraries. See §6 for the module-canvas design.
  - Rust remains the implementation language for the actual "OS kernel" logic (skill
    execution, memory router, scheduler) either way, so Tauri lets that logic live in the
    same language as the shell without a second runtime.
- **SQLite** (via `rusqlite`, bundled) for structured, mutable runtime state that the
  scheduler and UI both need to read and write concurrently (once implemented: skill run
  history, routine definitions and their next-fire timestamps).
- Plain files (TOML for config, Markdown for skills and memory) wherever the data is meant to
  stay human-editable and git/version-control friendly; the filesystem is deliberately kept
  as the single source of truth for that content rather than mirroring it into SQLite.

## 3. Workspace and crate layout

Axiomata-OS is a single Cargo workspace (edition 2024). Current members:

```
Axiomata-OS/
  Cargo.toml                     # workspace manifest; shared deps under [workspace.dependencies]
  crates/
    axiomata-core/                # the actual "OS" engine — no Tauri or macOS dependency
    axiomata-macos/                # boundary for future macOS-specific integration (stub)
    axiomata-cli/                   # headless binary that exercises axiomata-core end to end
  apps/
    dashboard/
      src-tauri/                     # the Tauri shell (package name "dashboard")
  docs/architecture.md                # this document
```

### `axiomata-core`

The platform-independent engine. It depends only on `home`, `serde`, `toml`, `thiserror`, and
`rusqlite` — no Tauri, no macOS APIs — so it can in principle be embedded in a headless binary
on another platform later. Its modules, as declared in `crates/axiomata-core/src/lib.rs`:

- `paths` — resolves Axiomata-OS's own runtime data directory. **Implemented.**
- `config` — loads/saves `~/.axiomata/config.toml`. **Implemented.**
- `db` — SQLite connection setup and schema migrations. **Implemented.**
- `error` — the crate-wide `AxiomataError` type (`thiserror`-based). **Implemented.**
- `agents` — agent backend dispatch (Claude Code / Ollama). **Module stub, planned for M1.**
- `skills` — skill discovery and headless execution. **Module stub, planned for M1.**
- `memory` — `CLAUDE.md` router file generation and staleness tracking.
  **Module stub, planned for M2.**
- `routines` — cron-like scheduled skill/prompt execution.
  **Module stub, planned for M3.**

The crate exposes a single top-level type, `AxiomataCore` (see §5), constructed via
`AxiomataCore::init()`.

### `axiomata-macos`

A boundary crate for future macOS-specific integration (e.g. Mail/Calendar access via
`osascript` subprocesses or EventKit through `objc2`). Currently an untouched template stub
(`pub fn add(left: u64, right: u64) -> u64`, generated by `cargo new --lib`) with no
Axiomata-specific code yet.

### `axiomata-cli`

A minimal binary (`crates/axiomata-cli/src/main.rs`) whose only job is to call
`axiomata_core::AxiomataCore::init()` and print the resulting paths, or print an error and
exit with a non-zero status on failure. It exists specifically so the core engine can be
exercised end to end — verifying that initialization actually works — without going through
the GUI at all. Run it with `cargo run -p axiomata-cli`.

### `apps/dashboard/src-tauri`

The Tauri shell, Cargo package name `dashboard` (crate name `dashboard_lib`, since the binary
and library names must differ on some platforms). Depends on `axiomata-core` via a path
dependency (`crates/axiomata-core`, referenced as `../../../crates/axiomata-core`). Its
`.setup()` hook, in `src/lib.rs`, calls `AxiomataCore::init()` once at startup and stores the
result as Tauri-managed state (`Mutex<AxiomataCore>`), so that future `#[tauri::command]`
handlers (M1+) can reach the same live config and database connection instead of
re-initializing. Today the only registered command is the scaffolded `greet` example from
the Tauri Vanilla-TypeScript template — no Axiomata-specific commands exist yet, and the
window itself is the unmodified template UI, not the planned dashboard.

## 4. Two separate data locations

A design decision that is easy to get wrong, so it is called out explicitly: Axiomata-OS
data lives in **two distinct places** with two distinct ownership models.

### `~/.axiomata/` — app-owned data

Everything that belongs to the application itself, independent of which Second-Brain
workspace the user currently has configured:

- `config.toml` — the app config (see §5)
- `axiomata.db` — the SQLite database
- `logs/` — JSONL run/routine logs (planned; empty today)
- `skills/` — **global** skills, i.e. built-in, always-available skills regardless of which
  workspace is active (e.g. a generic "clean up" SOP) — planned for M1, directory created but
  unused today

This directory is resolved by `crates/axiomata-core/src/paths.rs::axiomata_home()`. It
defaults to `~/.axiomata` and is deliberately a visible dotfolder (mirroring `~/.claude`)
rather than a hidden OS-convention path like `~/Library/Application Support/...`, because
Axiomata-OS is meant to be inspected by hand. It can be overridden via the `AXIOMATA_HOME`
environment variable, which both the test suite (to avoid touching a developer's real
`~/.axiomata`) and anyone who wants to run an isolated second instance can use.

### `workspace_root` — the user's Second-Brain workspace

A freely chosen, freely relocatable folder (`config.workspace_root` in `config.toml`,
defaulting to `~/Axiomata-Workspace`) that holds the user's actual "Second Brain" content:

- The memory router's root `CLAUDE.md` and per-area `CLAUDE.md` index files (planned for M2)
  — these must live inside the workspace, not inside the app's own data directory, because
  they only function as usable context when a Claude Code session is actually run with this
  folder as its working directory.
- **Workspace-local** skills under `<workspace_root>/.claude/skills/` (planned for M1) —
  skills that are specific to this particular Second Brain rather than globally useful.

### Why the split, and how they interact

The rationale: application bookkeeping (config, logs, the SQLite database, globally reusable
skills) should exist and stay stable independent of which Second Brain is currently open,
while the Second-Brain content itself needs to live where Claude Code's own context-loading
conventions expect it — inside the workspace directory being worked in. Axiomata-OS
deliberately does not invent a parallel file format for skills; it reuses Claude Code's own
project-vs-user-level skill convention (`~/.claude/skills/` vs. `.claude/skills/`) mapped onto
its own two locations. Once the skills registry is implemented (M1), it will scan **both**
directories and merge the results, with the workspace-local skill winning on a name
collision.

## 5. What is actually implemented today

This section describes only code that exists and runs; see §6 for planned work.

### `AxiomataCore::init()`

`crates/axiomata-core/src/lib.rs` defines `AxiomataCore { config: Config, db: Connection }`,
constructed by the single entry point `AxiomataCore::init()`. Both `axiomata-cli` and the
Tauri `.setup()` hook call exactly this function; no other initialization path exists. Its
steps, all idempotent (safe to call on every app start, including when everything already
exists):

1. Load `~/.axiomata/config.toml` via `Config::load()`. If the file didn't already exist,
   write the just-loaded default config back to disk immediately, so the file exists on disk
   for the user to inspect or edit from the very first run.
2. Create `~/.axiomata/logs/`, `~/.axiomata/skills/`, and the configured
   `workspace_root` directory, each via `fs::create_dir_all` (a no-op if already present).
3. Open `~/.axiomata/axiomata.db` and apply any pending SQLite migrations via
   `db::open_and_migrate()`.

If any step fails, `init()` returns an `AxiomataError` (see below) rather than panicking,
except for two narrow, deliberate cases: resolving the OS home directory itself
(`paths::axiomata_home()`, `config::default_workspace_root()`) panics if the OS cannot report
one at all, since Axiomata-OS has no meaningful way to run without a home directory.

### Configuration (`config.rs`)

`Config` is a `serde`-derived struct with two fields: `workspace_root: PathBuf` (default
`~/Axiomata-Workspace`) and `agents: AgentDefaults` (currently just `ollama_model: String`,
default `"llama3.2"`, unused until the Ollama backend exists). `Config::load()` returns the
default config if `~/.axiomata/config.toml` doesn't exist yet, or parses the file with
`toml::from_str` and returns `AxiomataError::ConfigParse` on malformed TOML.
`Config::save()` serializes with `toml::to_string_pretty` and writes it, creating the parent
directory first if needed.

### Path resolution (`paths.rs`)

Pure functions with no side effects, deriving every app-data path from a single root:
`axiomata_home()`, and from that `config_path()`, `db_path()`, `logs_dir()`,
`global_skills_dir()`. `axiomata_home()` checks the `AXIOMATA_HOME` environment variable
first and falls back to `home::home_dir().join(".axiomata")`.

### Database and migrations (`db/mod.rs`)

Uses `rusqlite` with the `bundled` feature (statically links SQLite, no system dependency).
`open_and_migrate()` opens (creating if needed) `~/.axiomata/axiomata.db`, unconditionally
ensures a `schema_version(version INTEGER)` bookkeeping table exists, reads the current max
applied version, and then runs every migration in the `MIGRATIONS` list
(`(u32, &str)` pairs of version number and SQL, embedded via `include_str!`) whose version is
greater than that, recording each as it applies. Migrations are append-only by convention:
once released, a migration's SQL is never edited or reordered.

The only migration today is `0001_init.sql`
(`crates/axiomata-core/src/db/migrations/0001_init.sql`), which creates a minimal
`app_meta(key TEXT PRIMARY KEY, value TEXT NOT NULL)` key/value table. It isn't consumed by
anything yet; its purpose in M0 is to prove the migration runner applies arbitrary DDL (not
just the bookkeeping table) and to give later milestones a place to persist small app-level
facts without a schema change.

### Errors (`error.rs`)

`AxiomataError` (via `thiserror`) covers every failure mode the above surfaces: `Io` (with
the offending path), `ConfigParse`, `ConfigSerialize`, `Database` (wraps `rusqlite::Error` via
`#[from]`), and `Migration` (tags the failing migration's version).

### `axiomata-cli`

Calls `AxiomataCore::init()` and prints the resolved workspace root, config file, database,
logs directory, and global skills directory paths on success; prints the error and exits 1 on
failure. This is the primary way to verify the core works without launching the GUI.

### The Tauri shell

`apps/dashboard/src-tauri/src/lib.rs`'s `run()` function builds the Tauri app, registers the
`tauri-plugin-opener` plugin and the scaffolded `greet` command, and in `.setup()` calls
`AxiomataCore::init()` (panicking via `.expect()` on failure — there is currently no
in-app error UI for a failed core initialization) and stores it as `app.manage(Mutex::new(core))`.
Beyond that, the app is the unmodified Tauri Vanilla-TypeScript template: the window,
`greet` command, and frontend are all template boilerplate, not yet replaced with any
Axiomata-specific dashboard UI.

## 6. What is designed but not yet implemented

The following is **planned design**, captured here because the rationale lives outside this
repository (in the owner's local planning notes) and should not be lost. None of it exists in
running code yet — every item below is a module stub with only a doc comment stating which
milestone will implement it. Do not mistake anything in this section for shipped behaviour.

### Agent backends (`agents/`) — planned for M1

Skill and routine execution will dispatch through a small Rust `enum`, not a plugin registry
or trait-object abstraction — a deliberate choice, since only two backends are needed today
and a generic multi-CLI abstraction would be premature generalization:

```rust
enum AgentBackend {
    ClaudeCode,
    Ollama { model: String },
}
```

- `AgentBackend::ClaudeCode` (`agents/claude_code.rs`) will spawn the headless Claude Code CLI
  (`claude -p "<prompt>"`) via `tokio::process::Command`, with the working directory set to
  the target skill's `cwd` (typically `workspace_root`) and a timeout, capturing stdout,
  stderr, exit code, and duration.
- `AgentBackend::Ollama { model }` (`agents/ollama.rs`) will talk to a local Ollama daemon's
  HTTP API directly (via the `ollama-rs` crate, default `http://localhost:11434`) — no
  subprocess, lower latency, intended for simple tasks like appending a to-do item.
- Skills and routines will each store only a plain string (`"claude-code"` or `"ollama"`);
  a `resolve()` function in `agents/mod.rs` will map that string (plus a configured Ollama
  model name) onto the concrete enum variant. If a further backend is ever needed (e.g.
  `opencode`, or a custom `rig`-based agent), the enum can gain a variant later without
  reworking the runner or scheduler — but no such backend is currently planned.

### Skills (`skills/`) — planned for M1

- `registry.rs` will scan **both** data locations described in §4 —
  `~/.axiomata/skills/*/SKILL.md` (global) and `<workspace_root>/.claude/skills/*/SKILL.md`
  (workspace-local) — and merge them into one list, with workspace-local skills winning on
  name collisions. Skill metadata (`name`, `description`, `model`, `effort`, `trigger`,
  `backend`) will be read from each `SKILL.md`'s frontmatter. The filesystem stays the single
  source of truth; skills are not planned to be duplicated into the database.
- `runner.rs` will resolve a skill's `backend` field via `agents::resolve()` and call
  `AgentBackend::run(...)`, with a timeout, capturing stdout/stderr/exit code/duration.
- `runlog.rs` will record every run both as a JSONL line in `~/.axiomata/logs/runs.log` (for
  `tail -f`-style inspection) and as a row in a SQLite `runs` table (for UI queries).

### Memory router (`memory/`) — planned for M2

- `walker.rs` will walk `workspace_root` using the `ignore` crate (respecting `.gitignore`)
  and group files by top-level area.
- `router.rs` will deterministically render router content into a clearly delimited block
  (`<!-- AXIOMATA-ROUTER:START/END -->`) inside the root `CLAUDE.md` and each area's
  `CLAUDE.md`, leaving the rest of each file untouched. Sync is planned to be **explicit**
  (a CLI command or a "Sync Now" UI action, or once at app start) rather than automatic on
  every file change, to avoid constant diff noise.
- `watcher.rs` will use the `notify` crate to flag the workspace as "stale" when a tracked
  file changes after the last sync, without triggering an automatic re-sync itself.

### Routines scheduler (`routines/`) — planned for M3

- `model.rs` will define the `Routine` and run-status data types.
- `store.rs` will provide SQLite CRUD for a planned `routines` table
  (`id, name, cron_expr, target_type, target, backend, enabled, next_fire_at, ...`) and a
  `routine_runs` table.
- `schedule.rs` will handle cron expression parsing and next-fire-time calculation (via the
  `cron` crate).
- `scheduler.rs` will run a single Tokio background task that polls every ~30 seconds,
  executes due routines through the same skill-runner path (or a raw prompt against the
  configured backend), logs the result, and recomputes `next_fire_at`. Planned restart
  behaviour: `next_fire_at` is always loaded from the database, never recomputed from
  scratch on startup, so a restart cannot cause a routine to double-fire; a `next_fire_at`
  that has already passed at startup deliberately does **not** trigger a catch-up run — only
  a recalculation and a "missed" log entry.

### Dashboard UI — module canvas, phase TBD

The current window is the unmodified Tauri template.

The intended UI is **not** a fixed layout but a free-form canvas (a "board"). The user
places **modules** onto it — self-contained widget tiles, each with its own frontend and
its own backend logic — and freely positions and resizes them; the layout is persisted.
The same module type can be placed **multiple times**, each instance carrying its own
configuration (e.g. two ToDo tiles bound to different files). Each tile works like a card:
the front shows content and interaction, the back shows that instance's settings, toggled
via a corner control (iOS-widget style). Modules integrate agents both actively ("have an
agent do this item") and passively (an agent infers from user activity whether an item can
be marked done). The particle-graph "Second Brain" visualization from `Screenshots/`
(D3-force or three.js, fed by the memory router) is one such module type, occupying the
centre.

Foundational ordering: the **module platform** is built first — the canvas with
drag/resize, layout persistence, and a clear module contract (what a module must provide,
how front/back and per-instance config work). Individual modules (ToDo, Email, Calendar,
Skills, Routines, Memory, a file viewer/editor, the particle graph, …) are then
interchangeable building blocks on top, each designed and implemented separately, step by
step.

All of this is **phase 2+**, out of scope for the milestones below: M1–M4 deliver backend
capability with only minimal placeholder UI. This module-canvas design supersedes the
earlier "three fixed panels, plain list/table" sketch.

### `axiomata-macos` — planned, phase TBD

Reserved as the integration boundary for macOS-specific features such as Mail or Calendar
access (likely via `osascript` subprocesses or EventKit through `objc2`). No design or
implementation exists yet beyond the empty crate scaffold.

## 7. Milestone status

- **M0 — Workspace scaffold: done.** Cargo workspace with all crates in place, the Tauri app
  scaffolded from the official Vanilla-TypeScript template with a path dependency on
  `axiomata-core`. `cargo build --workspace` and `cargo clippy --workspace -- -D warnings`
  are clean; `cargo tauri dev` opens a window; the first run of either the CLI or the Tauri
  app creates `~/.axiomata/{config.toml, axiomata.db, logs/, skills/}` with migrations
  applied and a default `workspace_root` created.
- **M1 — Skills runner, end to end (including agent backends and the two skill locations):
  not started.** Will land the `agents` enum and `resolve()`, the merged skills registry, the
  runner and run log, the corresponding Tauri commands, a minimal skills UI, and example
  skills for both backends.
- **M2 — Memory router: not started.** Will land the workspace walker, the deterministic
  `CLAUDE.md` router renderer, `sync_memory`/`get_memory_status`, and the stale-flag watcher.
- **M3 — Routines scheduler: not started.** Will land the routine store, cron scheduling, the
  poll loop, the corresponding Tauri commands, and a minimal routines UI.
- **M4 — Always-on behaviour: deferred, not part of the current milestones.** The MVP runs
  as an ordinary desktop app: the user starts and quits it manually, and closing the window
  ends the process — which also stops the scheduler, so routines (M3) only fire while the
  app is running. Autostart (`tauri-plugin-autostart`), single-instance guarding
  (`tauri-plugin-single-instance`), close-to-hide window behaviour, and a background /
  always-on scheduler are a later phase. `tauri-plugin-single-instance` may be pulled in
  early as a trivial safeguard against a second process double-firing the scheduler.

Each milestone from M1 onward is broken down into a detailed, step-by-step implementation
plan shortly before it is actually started, rather than all at once up front, so the detailed
planning stays in sync with the actual state of the code.
