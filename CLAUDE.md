# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project status

Axiomata-OS is an early-stage Rust + Tauri desktop app: a personal "Agentic OS" command
centre / second brain, built around the **ARMS framework** (Applications, Routines,
Memory, Skills — see `ARMS-Agentic-OS-Guide.pdf` for the original inspiration, though the
actual design has since diverged from it in several places).

Milestones **M0 (scaffold)**, **M1 (skills runner)**, **M2 (memory router)**,
**M3 (routines scheduler)**, **M5 (module-canvas dashboard)** and **M6 (particle graph /
Second Brain)** are complete: the
`agents` enum, the single-location skills registry + runner + run log, the `memory` router
(walker / renderer / `sync` / `status`), the `routines` module (cron `schedule` / `store` /
a 30 s Tokio poll loop in `scheduler`), and the Svelte module canvas (free-form tiles with
drag / resize / flip, layout persistence, four modules, an agentic chat bar, an
agent-callable module bridge, themes + custom CSS — see "Dashboard (M5)" below), plus the
workspace graph behind the tiles and its full-screen Second Brain view (see "Second Brain
(M6)"). Still unimplemented: always-on/background scheduling (M4). The full architecture rationale and
the milestone plans live in `~/.claude/plans/ich-m-chte-ein-agentic-shimmering-gadget.md`
(M0–M4) and `~/.claude/plans/toasty-inventing-crayon.md` (M5; M6 was planned in
conversation, see the owner's memory notes), outside this repo on the
owner's machine — read them before starting new work here if available; this file covers
what's needed to just build/run/navigate the repo as it stands.

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
cargo run -p axiomata-cli -- routines list  # scheduled routines, soonest next-fire first
cargo run -p axiomata-cli -- routines add --name daily --cron '0 0 9 * * *' --skill <name>
cargo run -p axiomata-cli -- routines tick  # run one scheduler poll pass now (no 30s wait)
cargo run -p axiomata-cli -- assistant "hi" [--resume <session_id>] [--instruct]  # one chat turn
cargo run -p axiomata-cli -- modules        # print the module manifest the dashboard wrote
cargo run -p axiomata-cli -- module-action <instance> <action> --json '{}'  # needs a running dashboard
cargo run -p axiomata-cli -- graph          # workspace graph summary (areas, links, skills, routines)
cargo run -p axiomata-cli -- import obsidian <folder> [--dry-run] [--skip-secrets]  # agent-sorted import

cd apps/dashboard && cargo tauri dev       # run the desktop app (hot-reloading dev mode)
cd apps/dashboard && npm run check         # svelte-check + tsc (must be clean)
cd apps/dashboard && npx vite --port 1420  # frontend alone in a browser: Tauri commands are
                                           # served by src/core/devmock.ts fixtures (DEV only)
```

For browser-level checks (`agent-browser` against `vite --port 1420`) the mock backend
returns fixture data; anything that needs the real Rust side (persistence, file commands,
the agent) is verified by launching `cargo tauri dev` under a scratch `AXIOMATA_HOME`.

One-time setup for the Tauri app: `cargo install tauri-cli --version "^2" --locked`, and
`cd apps/dashboard && npm install`.

## Architecture

Cargo workspace (edition 2024), members:
- `crates/axiomata-core` — the actual "OS" engine. No Tauri or macOS dependency, so it can
  in principle run headless elsewhere later. Modules: `paths`, `config`, `db`, `error`,
  `agents`, `skills`, `memory`, `routines` — all implemented.
- `crates/axiomata-macos` — boundary for future macOS-specific integration (e.g. Mail/
  Calendar access). Untouched stub.
- `crates/axiomata-cli` — thin binary that calls `axiomata_core::AxiomataCore::init()` and
  prints status. Exists specifically so the core can be exercised end-to-end without
  going through the GUI.
- `apps/dashboard/src-tauri` — the Tauri shell (package name `Axiomata-OS` — chosen to
  match exactly what shows in the macOS menu bar during `cargo tauri dev`, since a dev
  run has no bundled `.app`/`Info.plist` for `productName` to apply to and the OS just
  shows the compiled binary's own name; the `[lib]` target stays `dashboard_lib`,
  independent of the package name), depends on
  `axiomata-core` via a path dependency. Its `.setup()` hook (`src/lib.rs`) calls
  `AxiomataCore::init()`, kicks off a best-effort memory sync, starts the routine scheduler
  (`tauri::async_runtime::spawn(routines::serve(…))`, stop handle managed), and stores the
  `AxiomataCore` as managed state for the Tauri commands. `AxiomataCore` holds `config`
  unlocked and only `db` behind a `Mutex`, wrapped in an `Arc` so the scheduler task can
  hold its own handle. Plugins: `tauri-plugin-opener` and `tauri-plugin-window-state`
  (`StateFlags::SIZE | POSITION | MAXIMIZED` — the window remembers its geometry across
  restarts in `window-state.json` in the OS app-config dir, *not* `~/.axiomata/`; the
  `tauri.conf.json` `width`/`height` are only the first-run defaults). Both plugins'
  `*:default` permission sets are in `capabilities/default.json`.

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
files with an extracted (and sanitised) title. `memory::sync` regenerates them (deterministic
— a no-op sync is byte-identical), stamping `~/.axiomata/memory-last-sync.json` (per-workspace
timestamp map). `memory::status` reports staleness = a tracked file changed after that
marker. `upsert_block` never touches bytes outside the line-wise markers, writes atomically,
and refuses a symlinked target. `sync` refuses a home/`/` workspace root and reports per-file
failures in `SyncReport.failed` instead of aborting. Startup sync runs on a background
thread; no file watcher — the 3 s status poll re-walks.

The `routines` module (M3) fires cron-scheduled routines — a named skill or a raw prompt —
unattended. `routines::store` owns the `routines` / `routine_runs` tables (migration 0003);
`next_fire_at` is persisted and authoritative (never recomputed from the cron on load).
`routines::schedule` wraps the `cron` crate (**6–7 field**, seconds first: `0 */2 * * * *`).
`routines::scheduler::tick` is one poll pass (fire due routines exactly once, no backlog
replay); `serve` is the 30 s loop; `spawn` is the Tokio-context wrapper. A firing is
recorded as a normal `runs` row plus a `routine_runs` row linking to it. On startup
`reconcile_missed` rolls past-due routines forward with a `Missed` history row — **no**
catch-up fire. Routines fire only while the app or `axiomata-cli routines tick` runs
(always-on is M4). `RunRecord` / `RunStatus` / `RunSummary` now live in `skills::model`.

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

## Dashboard (M5)

Frontend: Svelte 5 + Vite + TS under `apps/dashboard/src/` — `core/` (stores, registry,
lifecycle, persist, commands, chat, staging, agent-bridge, backend types + `devmock`),
`canvas/` (Canvas, Tile, drag/resize actions), `shell/` (TopBar, IconBar, ModulePicker,
Settings, AssistantBar, ChatPanel, StagingLayer, Toasts), `modules/` (memory-status,
skills-deck, routines-board, md-file, todo, each `.svelte` + settings face, registered in
`modules/index.ts`; the graph's `second-brain` too), `themes/` (`tokens.css` = the `--ax-*` token template + one file per
theme: graphite, paper, steampunk, forest, ocean), `theme/validator.ts`. Every colour /
size goes through a `--ax-*` token; no literals in components.

- A module = `ModuleDefinition` (`core/types.ts`): type, title, inline-SVG icon, front
  component, optional settings component (flip side), default/min size, `singleton`,
  `stageable`, `actions[]`. Instances are mounted with a `ModuleContext` (`invoke`,
  reactive per-instance `config`, `emit`, `requestResize`).
- **Tile chrome** (`canvas/Tile.svelte`): the **front** face has **no chrome at
  rest** — no fill, no border, no shadow, no header divider — the module's
  content sits straight on the canvas, like the reference dashboard's edge
  panels. A caps section-label (icon + slightly-larger muted title) and faint top/bottom edge hairlines show; the grip glyph
  and ⚙/× buttons fade in on hover (`:focus-within` too). A faint fill
  (`color-mix` of `--ax-tile-glass-bg`) appears on hover so you can see what
  you're about to grab/resize, and the tile "materialises" fully
  (`--ax-tile-glass-bg` + `backdrop-filter` blur `--ax-tile-glass-blur` +
  drag shadow) while it is being moved/resized. The **back** (settings) face
  deliberately keeps the solid framed-window look (`--ax-surface-2` + 1px
  border + a bordered header bar + shadow) so it reads as a distinct surface.
  A tile is **dragged only from that top strip** — both headers carry
  `.tile-drag` and `use:draggable` gets `handle: ".tile-drag"` (`canvas/drag.ts`
  `DragOptions.handle`); the body no longer initiates a drag.
  `--ax-tile-glass-bg` is in the custom-theme allow-list (`theme/validator.ts`).
- **Persistence**: one hand-editable JSON file `~/.axiomata/dashboard.json` (layout +
  theme + per-instance config); the frontend owns the schema, Rust
  (`axiomata_core::dashboard`) only checks "object with numeric `version`", writes
  atomically (0600), and moves a corrupt file to `.bak`. Debounced 400 ms save on every
  store mutation.
- **Workspace files** (`axiomata_core::workspace`, commands `read/write_workspace_file`):
  relative to `config.workspace_root`, no `..`, must canonicalise inside the root,
  symlinks and hard links refused, ≤ 1 MiB, atomic O_EXCL temp + rename.
- **ToDo** (`todo` module, `singleton`): simple date-free tasks in one fixed file
  `ToDo.md` at the workspace root — **not** the (separate, future) Apple-Reminders
  module. All list logic is pure TS in `core/todo.ts` (+ `todo.test.ts`); the
  `.svelte` shell just loads / renders / writes back via `read/write_workspace_file`.
  Format is standard GFM task lists: open items under `# ToDo`, completed ones under a
  `## Done` heading stamped `- [x] … (done: YYYY-MM-DD)` (the date is for a later
  cleanup skill). The open/done split is that heading, regardless of each line's
  checkbox state, so hand-editing never moves an item. 5 s poll + reload; a write is
  skipped when byte-identical to the last read (same no-clobber trade-off as
  `md-file`, no deeper conflict detection). No new Rust, no migration. Bridge / `/todo`
  actions `add` · `complete` (first open item containing the given text) · `list` work
  statelessly on the file. `ToDo.md` shows up in the Second-Brain graph as an ordinary
  vault file, no special-casing.
- **HTML pages** (courses): the `md-file` module ("Document") frames `.html/.htm` read-only in
  a `<iframe sandbox="allow-scripts">` via **`srcdoc`** — raw content from `read_workspace_file`
  (the same guarded read every other file view uses), run through `core/htmllink.withNavIntercept`
  before being assigned. There is **no asset protocol** involved (removed 2026-09-04): the
  original design served lessons via `asset://` + `<iframe src=…>` (`open_workspace_html` +
  `core/backend.assetFileUrl`, granting the file's folder in the runtime asset scope), which
  looked correct on paper and even reported `is_allowed == true` on the Rust side — real fix
  attempts (`allow-same-origin`, a static `tauri.conf.json` scope entry) changed nothing — but
  every lesson rendered a reproducible blank white frame with "403 (Forbidden)" / "Not allowed
  to download due to sandboxing" in the WebKit console, jointly diagnosed with the owner via
  Safari's Web Inspector attached to the running app (matches reports against Tauri's
  asset/custom-protocol handling inside sandboxed iframes, e.g. tauri-apps/tauri#12767). `srcdoc`
  sidesteps the whole asset-protocol question — at the cost of two nuances `withNavIntercept`'s
  injected script handles by hand, since the browser's native behaviour gets both wrong for a
  `srcdoc` document (whose own URL is `about:srcdoc`, but whose *base URL* — what a relative
  `href` resolves against — is inherited from the **embedding app**, per the HTML living
  standard): (1) a same-folder link (`href="0003-next.html"`) has nothing of the lesson's real
  location to resolve against, so it's intercepted and posted to the parent via `postMessage`,
  which resolves it with `core/htmllink.resolveRelativeLink` (the WHATWG `URL` algorithm, the
  same approach validated for `assetFileUrl`) and re-opens the module on that path; (2) a
  same-page anchor (`href="#etappe-a"`) is worse than merely unresolvable — WebKit resolves it
  against the *app's* base URL and actually navigates the iframe to
  `http://localhost:1420/#etappe-a` (the dashboard shell itself), which then fails on CORS
  (opaque `null` origin) and renders blank — so anchors are intercepted too, handled with a
  plain `getElementById` + `scrollIntoView`, never letting the browser's own anchor-navigation
  run. `assetFileUrl` (in `core/backend.ts`) is unused now but left in place — general-purpose,
  tested, documented — in case a future feature needs a real `asset://` URL for something an
  iframe isn't involved in. Known limits: external links and anything with a URL scheme
  (`http:`, `mailto:`, `javascript:`, …) are left alone and simply do nothing, since the frame
  has no `allow-top-navigation` and no live network access beyond what `srcdoc` inlines.
- **Chat**: the bottom bar routes input — registered `/command` runs locally
  (`core/commands.ts`), other `/text` is a one-shot `instruct` turn, plain text a `chat`
  turn. `agents::claude_code::chat` = `claude -p --output-format json --permission-mode
  dontAsk|acceptEdits [--resume <id>]`, cwd = workspace root, the module manifest appended
  via `--append-system-prompt-file`. Markdown replies go through `core/markdown.ts`
  (marked + DOMPurify allow-list; no `data:` hrefs, raster-only `data:` images).
- **Agent → module bridge** (`axiomata_core::bridge`): the dashboard writes
  `~/.axiomata/module-context.md` (mounted instances + actions + how to call the CLI);
  the agent calls `axiomata-cli module-action <instance> <action> --json …`, which drops
  `~/.axiomata/module-actions/inbox/<id>.json`; the dashboard polls every 3 s, runs the
  action in the frontend, answers in `outbox/`; the CLI exits 2 on timeout. The manifest
  is appended to **every** Claude Code run — chat turns, skill runs and cron-fired
  routines alike (`AgentRequest.system_prompt_file`) — whenever the file exists, i.e.
  whenever the dashboard has run at least once; delete `module-context.md` to opt out.
- **Model**: every `claude -p` run passes `--model` from `config.agents.claude_model`
  (default `claude-sonnet-5`; a skill's frontmatter `model:` wins; empty = CLI default).
  Model names are validated against a flag-safe alphabet before reaching the command line.
- **Canvas physics** (`canvas/snap.ts`, pure + tested): tiles snap to `--ax-grid` (16 px) and
  magnetically to neighbour edges within 8 px (touch beats align, neighbour beats grid;
  `settings.snapEdges`), never overlap after a drop / resize (only the moved tile yields,
  bounded push-out then grid spiral). While dragging / resizing, `alignmentGuides` also
  emits a guide line wherever the moved tile shares an edge **or centre** with any other
  tile within `ALIGN_PX` (3 px) — Figma-style, at any distance, purely visual (separate
  from the 8 px magnet, which barely ever fires between far-apart tiles). `Tile.svelte`
  `showGuides` merges the two sets; `Canvas.svelte` draws each as a full-span accent line
  with a glow, above the dragged tile. Each tile carries an `anchor` (nearer edges + the
  canvas size at commit); the displayed position follows the anchored edge and is clamped
  into view (`displayRect`, never persisted) — so shrinking pulls tiles in, growing
  restores them, right/bottom tiles track their edge. Dot grid hidden unless
  `settings.showGrid`.
  The assistant bar is a centred pill (`--ax-assistant-width`), the chat panel
  `--ax-chat-width`; slide-ins use `--ax-dur-slow`. The owner works on a 21:9 monitor —
  never span the full width.
- **Themes**: `<html data-theme="…">`; a user `~/.axiomata/theme.css` is validated
  (`:root { --ax-*: … }` only) before injection; template via Settings → Copy template.

## Second Brain (M6)

- **Data**: `axiomata_core::graph::build` (command `get_workspace_graph`) — every tracked
  file (memory walker) with area / title / bytes / mtime (`.md` titles from frontmatter /
  heading, `.html` from `<title>`), `[[wiki]]`, relative Markdown links and relative HTML
  `href`s resolved to file paths, skills and routines as node kinds, the root `CLAUDE.md` as
  hub; capped at 5000 files (`truncated`). The owner's courses live in `vault/Learning/…`
  (a `.ignore` there hides tooling from the walker).
- **Frontend** `apps/dashboard/src/graph/`: `model.ts` (nodes / edges / area segments,
  theme-derived colours, `regroup` by folders, `searchNodes`, `neighbours`), `layout.ts`
  (Rings: skills inner ring, files on arcs inside their area segment, routines outer ring;
  Circle; **Hex**: hub/skills/routines/area markers placed exactly as in Rings, but every
  file gets its own flat-top hex cell instead of a dot — a hex-ring spiral out from the
  origin (`hexRing`/`hexToPixel`, standard axial-coordinate hex-grid math), cells handed
  out per area in that area's existing angular wedge so the mosaic still reads as "areas
  around a hub", one shared cell size (`GraphModel.hexUnit`, graph units) solved from the
  file count and rescaled to land exactly on Rings' file band. `layoutOrbit` (skills,
  routines and the newest notes as icon nodes on the rim, every file as a point of a 3-D
  fibonacci-sphere cloud) is a separate export used only by the dashboard-centre background
  widget directly (not part of `LayoutKind`/`applyLayout` any more — Second Brain's
  full-view switcher dropped Orbit as an option 2026-09-04, kept only as the home-screen
  background), `render.ts`
  (Canvas 2D, DPR-aware, spin, hover hit-test, view transform, highlight; `mode: "rings"`
  draws ring captions SKILLS / MEMORY / ROUTINES at 12 o'clock and per-segment counts
  instead of outer area names, `mode: "hex"` is the same draw pass with file nodes drawn as
  hexagons (`drawHexCell`, sized from `hexUnit`) instead of circles, `mode: "orbit"` draws
  the dark disc with hex texture and rim, a geodesic wireframe, the spinning cloud and the
  icon ring with age badges). No graph library on purpose; `d3-force` would only be added
  for a force layout.
- **Nodes**: hub, **area** (one per folder, labelled like the folder, on its own ring
  between skills and files), file, skill, routine — each non-file kind carries a glyph
  (hexagon / folder / bolt / clock, `graph/Legend.svelte` explains them; the file swatch
  itself switches from a dot to a small hexagon when the Hex layout is active).
- **Module `second-brain`**: `singleton` + `background` — mounted full-size in
  `#particle-slot` behind the tiles by `canvas/BackgroundHost.svelte` (corner ⚙ / ×; always
  Orbit, not user-switchable there). Click → `open-second-brain` bus event →
  `shell/SecondBrainView.svelte` (full screen:
  pan / zoom, search, Rings / Circle / Hex, Areas / Folders, detail panel with View here /
  Copy path / Fly to / Run skill / toggle routine, content preview via
  `core/markdown.ts` `excerpt`/`excerptHtml`, links split into out / in, area notes
  grouped by subfolder; a `?` help block explains Rings / Circle / Hex, Areas / Folders,
  Rotation). Also `/brain [path | ? query]` and the module actions `open`, `search`,
  `refresh`. **Search**: the box matches titles / paths / areas locally and, debounced,
  note **contents** through `search_workspace` (`workspace::search`: case-insensitive,
  all words on one line, tags stripped, most hits first); a results list with snippets
  sits under the box (↑↓ / Enter jump to the node), non-matches are dimmed. View
  preferences live in `dashboard.json` → `settings.secondBrain`.
- **Import**: `axiomata_core::importer` + `axiomata-cli import obsidian` — notes
  normalised to "# Title + content" (frontmatter / tag lines dropped, tags only as hints),
  the agent proposes the areas and assigns every note in one JSON turn, files are written
  under `<workspace>/<Area>/`, never overwriting; secret-looking notes are flagged, not
  skipped, unless `--skip-secrets`.
- **New note**: `axiomata_core::notes` + the top bar's "New note" icon — no separate dialog;
  opens the `md-file` module itself in compose mode (`isNew` config, no `path` yet: a bare
  textarea, Save calls the new `create_note` command instead of `write_workspace_file`, then
  re-points `config.path` at the written file, which falls straight into the module's normal
  read-mode viewer). No title field either: a note either starts with its own `# Heading`
  (kept verbatim), or `axiomata_core::notes::placement_prompt` also asks the agent to propose
  one, via `memory::walker::first_heading` telling which case applies. A scaled-down
  `importer`: one note instead of a batch, and like `importer::assignment_prompt`,
  `placement_prompt` lets the agent propose a brand new top-level area when none of the
  vault's existing ones genuinely fit (that judgment call is the point of asking the agent at
  all), nudging toward reuse and broad/durable names first; `Inbox` is the last resort, not
  the only alternative to an existing area. The Rust side does the actual write, deduping on
  a name collision (`-2`, `-3`, …) rather than skipping like `importer::apply` does.
- Escape across overlays: the first handler that acts calls `preventDefault()`; later ones
  (Second Brain, chat) check `defaultPrevented` — never the DOM (outro transitions linger).

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
