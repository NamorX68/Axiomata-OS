# Axiomata-OS Architecture

This document describes the architecture of Axiomata-OS as it actually exists in this
repository today, and — clearly separated — the design that later milestones will build on
top of it. If you have never seen this repo before, this is the place to start after the
[README](../README.md).

> **Maintenance note:** this file is *not* auto-loaded into every Claude Code turn the way
> the repo's `CLAUDE.md` is — that's deliberate, so the detailed walkthrough below can live
> here without inflating every single request. It also means it only stays accurate if it is
> updated by hand. Between M3 and M6 it was not (the milestone summaries went into `CLAUDE.md`
> instead, since that file is what's guaranteed to be read, and this one quietly went stale).
> When a milestone or major feature lands, update **this file's** §5/§7, not just
> `CLAUDE.md`'s "Project status" paragraph.

## 1. Vision

Axiomata-OS is a personal "Agentic OS": a single desktop application that acts as
a command centre / second brain for one user. (The long-term goal is an always-on
application; the milestones below run it as an ordinary app the user starts and quits —
see §7, M4.) It is organized around the **ARMS framework**
(**A**pplications, **R**outines, **M**emory, **S**kills), the model described in
[`ARMS-Agentic-OS-Guide.pdf`](../ARMS-Agentic-OS-Guide.pdf) at the repository root. The core
idea: instead of scattering AI-agent usage across ad-hoc scripts and chat sessions, give it a
proper home with persistent state — reusable **skills** (packaged, reusable agent tasks),
a **memory** layer that keeps a personal knowledge workspace machine-readable for an agent,
**routines** that fire skills on a schedule without a human present, and **applications** —
deeper integrations with things like mail, calendar, and reminders.

The actual implementation has diverged from the PDF guide in a number of concrete design
decisions (documented below); the PDF should be read as inspiration, not as a specification
for this codebase.

## 2. Tech stack and why

- **Rust** for the core engine (`axiomata-core`). The scheduler, skill runner, and memory
  router are exactly the kind of long-running, I/O-heavy, concurrency-sensitive logic Rust
  is well suited for, and the core has no GUI dependency, so it can in principle run headless
  on another machine later.
- **Tauri** for the desktop shell, chosen deliberately over both **Electron** and a native
  **SwiftUI** app:
  - Electron was rejected primarily for its resource footprint (bundling a full Chromium
    per app instance) for what is meant to be an always-running background application.
  - Tauri uses the OS's native WebView instead of a bundled browser engine, which keeps the
    resource footprint far closer to a native app while still rendering the dashboard with
    ordinary web technology (Svelte, Canvas 2D) — the actual module-canvas + particle-graph
    UI described in §5 would be substantially harder to build in SwiftUI.
  - Rust remains the implementation language for the actual "OS kernel" logic (skill
    execution, memory router, scheduler) either way, so Tauri lets that logic live in the
    same language as the shell without a second runtime.
- **SQLite** (via `rusqlite`, bundled) for structured, mutable runtime state that the
  scheduler and UI both need to read and write concurrently: skill run history, routine
  definitions and their next-fire timestamps.
- **Svelte 5 + Vite + TypeScript** for the dashboard frontend — runes-based reactivity, no
  virtual DOM overhead, small bundle. Canvas 2D (not a graph library) draws the particle
  graph directly.
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
      src/                           # Svelte frontend (core/canvas/shell/modules/themes/graph)
      src-tauri/                     # the Tauri shell (package name "Axiomata-OS")
  docs/architecture.md                # this document
```

### `axiomata-core`

The platform-independent engine. Its dependencies are all cross-platform library crates
(`home`, `serde`/`serde_json`, `toml`, `thiserror`, `chrono`, `rusqlite`, `tokio`,
`gray_matter`, `ollama-rs`, `ignore`, `cron`) — no Tauri, no macOS APIs — so it can in
principle be embedded in a headless binary on another platform later. Its modules, as
declared in `crates/axiomata-core/src/lib.rs`:

- `paths` — resolves Axiomata-OS's own runtime data directory.
- `config` — loads/saves `~/.axiomata/config.toml`.
- `db` — SQLite connection setup and schema migrations.
- `error` — the crate-wide `AxiomataError` type (`thiserror`-based).
- `agents` — agent backend dispatch (Claude Code / Ollama), chat turns, the module bridge.
- `skills` — skill discovery, headless execution, and run logging.
- `memory` — `CLAUDE.md` router file generation and staleness tracking.
- `routines` — cron-scheduled skill/prompt execution via a background poll loop, full CRUD
  (create/edit/delete/enable-disable), and firing history.
- `dashboard` — validates the frontend-owned `dashboard.json` layout file (schema is opaque
  to Rust beyond "object with numeric `version`"), atomic writes.
- `workspace` — guarded read/write/search of files under `config.workspace_root`.
- `bridge` — the agent → module action bridge (file-queue based, see §5).
- `graph` — builds the workspace graph (files/areas/skills/routines/links) the Second Brain
  visualization renders.
- `importer` / `notes` — Obsidian-vault import and single-note creation, both agent-assisted
  area placement.

All implemented — there is no unimplemented `axiomata-core` module left from the original
milestone plan (see §7).

### `axiomata-macos`

A boundary crate reserved for future macOS-specific integration beyond what MCP servers
(Apple Mail / Reminders / Calendar, used today via Claude Code's own MCP tool-calling — see
§5 "Connector modules") already cover. Currently an untouched template stub with no
Axiomata-specific code.

### `axiomata-cli`

A `clap`-based binary whose job is to exercise `axiomata-core` end to end without the GUI:
`status`, `list-skills`, `run-skill`, `list-runs`, `memory sync|status`,
`routines list|add|edit|delete|enable|disable|history|tick`, `assistant` (one chat/instruct
turn, `--allowed-tools` for testing an MCP tool call before wiring it into the dashboard),
`import obsidian`, `graph`, `modules`, `module-action`. Run it with
`cargo run -p axiomata-cli -- <subcommand>`.

### `apps/dashboard/src-tauri`

The Tauri shell, Cargo package name `Axiomata-OS` (matches what the macOS menu bar shows
during `cargo tauri dev`, since a dev run has no bundled `.app`/`Info.plist`; the `[lib]`
target stays `dashboard_lib`). Depends on `axiomata-core` via a path dependency. Its
`.setup()` hook (`bootstrap.rs`) calls `AxiomataCore::init()`, kicks off a best-effort memory
sync on a background thread, starts the routine scheduler
(`tauri::async_runtime::spawn(routines::serve(…))`, stop handle managed), and stores the
`AxiomataCore` as managed state. `AxiomataCore` holds `config` unlocked and only `db` behind a
`Mutex`, wrapped in an `Arc` so the scheduler task can hold its own handle. Plugins:
`tauri-plugin-opener` and `tauri-plugin-window-state` (the window remembers its geometry
across restarts). See §5 for the full command surface and the Svelte frontend.

### `apps/dashboard/src` (frontend)

Svelte 5 + Vite + TS: `core/` (stores, registry, lifecycle, persist, commands, chat, staging,
agent-bridge, backend types + `devmock` for browser-only development), `canvas/` (Canvas,
Tile, drag/resize actions, snap physics), `shell/` (TopBar, IconBar, ModulePicker, Settings,
AssistantBar, ChatPanel, StagingLayer, Toasts, SecondBrainView), `modules/` (one `.svelte` +
optional settings face per module type, registered in `modules/index.ts`), `themes/`
(`tokens.css` + one file per theme), `graph/` (the particle-graph model/layout/render, shared
between the dashboard-centre background and the full-screen Second Brain view). Details in §5.

## 4. Two separate data locations

A design decision that is easy to get wrong, so it is called out explicitly: Axiomata-OS
data lives in **two distinct places** with two distinct ownership models.

### `~/.axiomata/` — app-owned data

Everything that belongs to the application itself, independent of which Second-Brain
workspace the user currently has configured:

- `config.toml` — the app config (agent backend defaults, model, `workspace_root`).
- `axiomata.db` — the SQLite database (skill runs, routines + their history).
- `logs/` — `runs.log` (JSONL skill-run mirror, 0600).
- `skills/` — **all** skills. Skills are application-level: always available regardless of
  which Second Brain is active, and managed only by the user. There is no second,
  workspace-local skill location — see "Why one skill location" below.
- `dashboard.json` — the frontend-owned canvas layout + theme + per-instance module config.
- `module-context.md` / `module-actions/{inbox,outbox}/` — the agent → module bridge (§5).
- `memory-last-sync.json` — the memory router's per-workspace staleness marker.

Resolved by `crates/axiomata-core/src/paths.rs::axiomata_home()`. Defaults to `~/.axiomata`,
deliberately a visible dotfolder rather than a hidden OS-convention path, because
Axiomata-OS is meant to be inspected by hand. Overridable via `AXIOMATA_HOME` (used by the
test suite and anyone running an isolated second instance).

### `workspace_root` — the user's Second-Brain workspace

A freely chosen, freely relocatable folder (`config.workspace_root`, defaulting to
`~/Axiomata-Workspace`) that holds the user's actual "Second Brain" content: the memory
router's `CLAUDE.md` index files, every note/area folder the workspace graph (§5) walks, and
fixed-path module files like `ToDo.md`. These must live inside the workspace, not the app's
own data directory, because the router files only function as usable context when a Claude
Code session actually runs with this folder as its working directory.

### Why the split

Application bookkeeping (config, logs, the SQLite database, skills, dashboard layout) should
exist and stay stable independent of which Second Brain is currently open. The Second-Brain
content itself needs to live where Claude Code's own context-loading conventions expect it.

### Why one skill location

An earlier design also read workspace-local skills from `<workspace_root>/.claude/skills/`
and let them override global ones. That was dropped: skills in Axiomata-OS are
application-level tasks, not per-vault content, and `<workspace_root>/.claude/skills/` is
exactly the kind of directory that receives synced / cloned / shared / agent-written files —
an untrusted-content path feeding a full agent run. Keeping skills solely in
`~/.axiomata/skills/`, a directory only the user manages, removes that exposure and the
merge/precedence machinery with it.

## 5. What is actually implemented today

This section describes only code that exists and runs; see §6 for planned work, §7 for the
milestone-by-milestone history.

### `AxiomataCore::init()`

`crates/axiomata-core/src/lib.rs` defines `AxiomataCore`, constructed by the single entry
point `AxiomataCore::init()`, called by both `axiomata-cli` and the Tauri `.setup()` hook —
no other initialization path exists. Fully idempotent: loads-or-creates `config.toml`,
creates `logs/`/`skills/`/the workspace root if missing, seeds the bundled `example-skill`
into `~/.axiomata/skills/` if absent (never overwrites), opens the SQLite DB and applies
pending migrations.

### Agent backends (`agents/`)

Execution dispatches through a small `enum`, `AgentBackend { ClaudeCode, Ollama { model } }`
— deliberately not a trait/registry (see §6). `AgentRequest` carries `prompt`, `cwd`,
`timeout`, `env`, `system_prompt_file` (the module bridge manifest, appended to **every**
Claude Code run whenever it exists), `model`, and `allowed_tools`.

- `claude_code.rs` spawns `claude -p --output-format json --permission-mode
  dontAsk|acceptEdits [--resume <id>] [--model …] [--allowedTools …]` via
  `tokio::process::Command`. The prompt goes on the child's **stdin**, never as an argv
  token, so a prompt beginning with `-` can't be parsed as a flag (routines fire this
  unattended). `--model` is validated against a flag-safe alphabet before reaching the
  command line; empty means CLI default.
- **`allowed_tools` and MCP tools — a real trap, found live building the Calendar module:**
  an MCP tool call (e.g. `apple-reminders`'s `calendar_events`) is **not** covered by
  `--permission-mode` at all. A headless `-p` run with no interactive approver **silently
  denies** the call — the run still exits 0 and "succeeds", the tool call is just refused,
  with no error surfaced anywhere. Any skill or chat/instruct turn that needs an MCP tool
  must set `allowed_tools` (skill frontmatter `allowed_tools:`, or the caller's own
  `--allowed-tools`/`ChatRequest.allowed_tools`) — there is no other way to reach one.
- `ollama.rs` makes one non-streaming `POST /api/generate` call to the local daemon.

### Skills runner (`skills/`)

Skills live in **one** place: `~/.axiomata/skills/<name>/SKILL.md`
(`registry::list_skills`/`find_skill`, frontmatter via `gray_matter`, filesystem is the only
source of truth). `runner::execute_skill` builds the prompt (`/<name>` for Claude Code, the
`SKILL.md` body for Ollama) and runs the backend without touching the database;
`execute_and_record_skill` adds the DB/log write. `runlog.rs` persists to the SQLite `runs`
table and appends JSONL to `logs/runs.log`. A skill whose SOP needs an MCP tool (a
"connector" skill, see below) must set frontmatter `allowed_tools:` — see the trap above.

### Memory router (`memory/`)

Keeps a generated, clearly delimited block between `<!-- AXIOMATA-ROUTER:START/END -->` in
the workspace's `CLAUDE.md` files (root + one per top-level "area"), listing folders/files
with an extracted, sanitised title. `sync` regenerates them (deterministic — a no-op sync is
byte-identical), stamping `~/.axiomata/memory-last-sync.json`; `status` reports staleness = a
tracked file changed after that marker. `upsert_block` never touches bytes outside the
markers, writes atomically, refuses a symlinked target. No file watcher — the dashboard's 3 s
status poll and this router's own on-demand walk are enough; sync itself stays explicit
(a button, a CLI command, or once at startup on a background thread).

### Routines scheduler (`routines/`)

A routine is a cron schedule (6–7 fields, seconds first, e.g. `0 */2 * * * *` — **not** the
5-field crontab format) bound to a named skill or a raw prompt, fired unattended by a 30 s
Tokio poll loop. `next_fire_at` is persisted and authoritative — never recomputed from the
cron on load — and moves forward in exactly one place, `store::advance`, called *before* the
target runs, so firings are **at-most-once**: a crash mid-fire drops that firing rather than
repeating it. A `next_fire_at` already past at startup is rolled forward without firing (a
`Missed` history row instead) — no catch-up replay. Full CRUD: `add` / `update` (full
replace, not a partial patch — every mutable field is resubmitted, matching `add`'s shape;
recomputes `next_fire_at` when staying enabled, leaves it untouched while staying disabled) /
`delete` (its `routine_runs` history cascades via the migration's `ON DELETE CASCADE` FK) /
`set_enabled`. Surfaced via `axiomata-cli routines *`, the `list_routines`/`add_routine`/
`update_routine`/`delete_routine`/`set_routine_enabled`/`routine_history` Tauri commands, and
the `routines-board` module (§5's "Modules" list) — whose "Add"/"Edit" form uses a friendly
interval picker (every-N-minutes / hourly / daily / weekly, plus a raw-cron escape hatch),
not a bare cron text field; `core/routineInterval.ts` converts between the two and only
recognises the exact shapes it itself generates, falling back to "custom" (verbatim cron)
for anything else rather than guessing. Routines only fire while the app or
`axiomata-cli routines tick` runs — always-on is M4 (deferred, §6).

### Errors (`error.rs`)

`AxiomataError` (via `thiserror`) covers every failure mode above and below: `Io`,
`ConfigParse`/`ConfigSerialize`, `Database`, `Migration`, agent errors
(`UnknownAgentBackend`/`AgentSpawn`/`AgentTimeout`/`AgentApi`), skill errors
(`InvalidSkill`/`SkillNotFound`), router errors (`InvalidRouter`/`UnsafeWorkspaceRoot`), and
routine errors (`InvalidRoutine`/`InvalidCron`/`CorruptRoutineRow` — a routine row that fails
to reconstruct is skipped from listings, not allowed to hide every other routine, and
separately surfaced via `list_corrupted`).

### Dashboard frontend — module canvas

The Tauri shell's frontend is a free-form canvas (a "board"), not a fixed layout. The user
places **modules** onto it — self-contained widget tiles, each with a front (content) and
back (settings) face flipped via a corner control — freely positions and resizes them
(layout persisted), and can place the same module type multiple times, each instance
carrying its own config.

- **Module contract** (`core/types.ts`): a `ModuleDefinition` (type, title, icon, front/back
  components, default/min size, `singleton`, `stageable`, `background`, `actions[]`); a
  mounted instance gets a `ModuleContext` (`invoke`, reactive per-instance `config`, `emit`,
  `requestResize`). Adding a module = one new file + one `registerModule(...)` call.
- **Tile chrome** (`canvas/Tile.svelte`): the front face has no chrome at rest (no fill,
  border, or shadow — content sits straight on the canvas); a faint fill + backdrop blur
  appears on hover/drag. The back face keeps a solid framed-window look. A tile is dragged
  only from its top strip (`.tile-drag` + `use:draggable` with a `handle`).
  **Canvas physics** (`canvas/snap.ts`): tiles snap to a 16 px grid and magnetically to
  neighbour edges within 8 px on drop/resize (never overlap — the moved tile yields via
  bounded push-out then grid spiral); while dragging, Figma-style alignment guide lines
  appear at 3 px against any edge or centre. Each tile carries an `anchor` (nearer edges +
  canvas size at commit) so shrinking the window pulls tiles in and growing restores them.
- **Persistence**: one hand-editable JSON file `~/.axiomata/dashboard.json`; the frontend
  owns the schema, Rust only validates "object with numeric `version`", writes atomically
  (0600), and moves a corrupt file to `.bak`. Debounced 400 ms save on every store mutation.
- **Workspace files** (`workspace.rs`, `read/write_workspace_file` commands): relative to
  `config.workspace_root`, no `..`, must canonicalise inside the root, symlinks/hard links
  refused, ≤ 1 MiB, atomic O_EXCL temp + rename.
- **Chat**: the bottom bar routes input — a registered `/command` runs locally
  (`core/commands.ts`), other `/text` is a one-shot `instruct` turn, plain text a `chat`
  turn. Markdown replies go through `core/markdown.ts` (marked + DOMPurify allow-list; no
  `data:` hrefs except raster images).
- **Agent → module bridge** (`axiomata_core::bridge`): the dashboard writes
  `~/.axiomata/module-context.md` (mounted instances + actions + how to call the CLI); the
  agent calls `axiomata-cli module-action <instance> <action> --json …`, which drops a file
  into `~/.axiomata/module-actions/inbox/`; the dashboard polls every 3 s, runs the action,
  answers in `outbox/`; the CLI exits 2 on timeout. Appended to every Claude Code run
  (chat, skill runs, cron-fired routines) whenever the manifest file exists.
- **Themes**: `<html data-theme="…">`, every colour/size through a `--ax-*` token (graphite,
  paper, steampunk, forest, ocean); a user `~/.axiomata/theme.css` is validated
  (`:root { --ax-*: … }` only) before injection.
- **Connector modules — "provider = skill, not code"**: Calendar and Reminders (and Mail)
  are the pattern for any future integration behind an MCP server the app doesn't have
  first-class Tauri commands for. A `*-digest` skill (not in this repo) reads the source via
  an MCP server's tools and replies with one JSON object; **there is no live poll** — data
  sits behind an MCP tool only an agent can reach, so every refresh is a real agent turn
  (whichever run happened most recently: by hand, on a schedule via a Routine, or the tile's
  own ↻, all the same `run_skill` mechanism). `core/skillRun.ts`'s `loadLatestSkillRun` (find
  the most recent run of skill X) and its `stripCodeFence` (the model adds a ` ```json ` fence
  despite the SOP saying not to) are shared connector infrastructure. **Writes** (create /
  complete / delete) go through a fresh, silent one-shot **instruct turn**
  (`core/instruct.ts`'s `runInstructWrite`) instead: a skill's SOP is fixed at authoring
  time and has no way to take a form's runtime parameters, so the instruction spells out the
  exact MCP tool call in the instruction text itself and the caller updates its already-loaded
  digest locally (no full re-run) from the reply.
- **HTML pages** (courses): rendered read-only via `<iframe sandbox="allow-scripts" srcdoc=…>`
  — **not** an `asset://` URL. An earlier `asset://` + `<iframe src=…>` design looked correct
  (even reported `is_allowed == true` on the Rust side) but every lesson rendered a blank
  white frame — "403 (Forbidden)" / sandboxing refusal in the WebKit console, matching known
  issues with Tauri's asset/custom-protocol handling inside sandboxed iframes. `srcdoc`
  sidesteps the whole asset-protocol question, at the cost of `core/htmllink.withNavIntercept`
  handling same-folder links and same-page anchors by hand (a `srcdoc` document's *base URL*
  for resolving relative `href`s is the embedding app, not the lesson's real location, per the
  HTML living standard).
- **Modules shipped today**: memory-status, skills-deck, routines-board (§5 above), md-file
  (Markdown + HTML viewer, also used as the compose surface for "New note"), todo (a flat
  `ToDo.md` checklist, GFM task lists, inline-editable), calendar, reminders, mail (connector
  modules per the pattern above), and second-brain (below) — each with a front and, where
  relevant, a settings face.

### Second Brain — particle graph

`axiomata_core::graph::build` (command `get_workspace_graph`) walks every tracked file
(reusing the memory walker) plus skills and routines as graph nodes, and every `[[wiki]]` /
relative Markdown link / relative HTML `href` as an edge, capped at 5000 files. The frontend
(`apps/dashboard/src/graph/`) renders this with Canvas 2D (no graph library) in three
full-view layouts (Rings, Circle, Hex — a hex-grid mosaic of file cells sized to match Rings'
band) plus a separate `layoutOrbit` used only by the dashboard-centre background widget (a
spinning 3-D fibonacci-sphere point cloud inside a wireframe geodesic, with skills/routines/
newest notes as an icon rim) — Orbit is the fixed name for that background widget
specifically, distinct from "Second Brain" (the full-screen view it opens into on click).
The full view (`shell/SecondBrainView.svelte`) adds pan/zoom, search (title/path/area
locally, note contents via `search_workspace`, debounced), and a detail panel (view / copy
path / fly-to / run skill / toggle routine). Import (`axiomata_core::importer` +
`axiomata-cli import obsidian`) and single-note creation (`axiomata_core::notes`, the "New
note" icon) both have the agent propose placement into one of the workspace's existing
top-level areas, or a brand-new one when none genuinely fit, in one JSON turn.

## 6. What is designed but not yet implemented

### Why the agent backend is an `enum`, not a trait

Skill and routine execution dispatches through `AgentBackend` (a two-variant `enum`), not a
plugin registry or trait-object abstraction — a deliberate choice, since only two backends
are needed and a generic multi-CLI abstraction would be premature generalization. If a
further backend is ever needed, the enum can gain a variant without reworking the runner or
scheduler — but no such backend is planned.

### M4 — always-on / background scheduling

The app still runs as an ordinary desktop app the user starts and quits; closing the window
ends the process, which also stops the routine scheduler. Autostart
(`tauri-plugin-autostart`), single-instance guarding (`tauri-plugin-single-instance`),
close-to-hide window behaviour, and a true background/always-on scheduler are this deferred
phase — the only milestone from the original M0–M4 plan not yet started.

### `axiomata-macos`

Reserved as an integration boundary for macOS-specific features beyond what MCP servers
already cover (§5's connector modules currently reach Apple Mail/Calendar/Reminders that
way). No design or implementation exists yet beyond the empty crate scaffold.

## 7. Milestone status

- **M0 — Workspace scaffold: done.** Cargo workspace with all crates in place, the Tauri app
  scaffolded with a path dependency on `axiomata-core`.
- **M1 — Skills runner, end to end: done.** The `agents` enum, the single-location skills
  registry, the runner, the run log, and a bundled `example-skill` seeded on first run.
- **M2 — Memory router: done.** The workspace walker, the deterministic router renderer,
  `memory::sync`/`memory::status`, staleness tracked via a marker file (poll-only, no file
  watcher).
- **M3 — Routines scheduler: done.** `routines/{model,schedule,store,scheduler}`, at-most-once
  firing via advance-before-execute, startup catch-up rolls forward without re-firing.
- **M4 — Always-on behaviour: deferred**, not part of the current milestones — see §6.
- **M5 — Module-canvas dashboard: done.** The Svelte module canvas (free-form tiles, drag/
  resize/flip, layout persistence), the initial four modules (memory-status, skills-deck,
  routines-board, md-file), the agentic chat bar, the agent ↔ module bridge, themes + custom
  CSS. All 13 build steps merged; the full frontend Vitest suite came with it.
- **M6 — Particle graph / Second Brain: done.** The workspace graph, Canvas-2D Rings/Circle/
  Hex layouts plus the dashboard-centre Orbit background widget, the full-screen Second Brain
  view (search, detail panel, prefs), Obsidian import and single-note creation with
  agent-assisted area placement. Force-layout and a 3-D polyhedron full view were considered
  and set aside.
- **Post-M6 feature work (ongoing, not numbered milestones):** the ToDo module (inline-editable
  GFM task list); the Calendar and Reminders connector modules (the "provider = skill, not
  code" pattern, §5) plus their write paths (instruct-turn based create/complete/delete); the
  `srcdoc`-based HTML/course viewer (replacing an `asset://` design that never actually
  worked); the Mail module; five selectable themes (graphite, paper, steampunk, forest,
  ocean); Routines CRUD in the UI itself — edit and delete, plus a friendly interval picker
  replacing the raw cron text field.

Each milestone from M1 onward was broken down into a detailed, step-by-step implementation
plan shortly before it was actually started, rather than all at once up front — those plans
live outside this repository, in the owner's local Claude Code planning notes, since their
value is in guiding the work in progress rather than as a permanent record once it lands.
