# Axiomata-OS Architecture

This document describes the architecture of Axiomata-OS as it actually exists in this
repository today, and — clearly separated — the design that later milestones will build on
top of it. If you have never seen this repo before, this is the place to start after the
[README](../README.md).

> **Maintenance note:** this file is *not* auto-loaded into every agent turn the way the
> repo's `AGENTS.md` is (Claude Code reads it through `CLAUDE.md`, which imports it) —
> that's deliberate, so the detailed walkthrough below can live here without inflating every
> single request. It also means it only stays accurate if it is updated by hand. Between M3
> and M6 it was not (the milestone summaries went into `CLAUDE.md` instead, since that file
> is what's guaranteed to be read, and this one quietly went stale). When a milestone or major
> feature lands, update **this file's** §5/§7, not just `AGENTS.md`'s "Project status"
> paragraph.

## 1. Vision

Axiomata-OS is a personal "Agentic OS": a single desktop application that acts as
a command centre / second brain for one user. (It runs as an ordinary app the user starts
and quits; an always-on variant was considered and then dropped outright — see §7, M4.)
It is organized around the **ARMS framework**
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
    axiomata-macos/                # macOS integration boundary; today only the pasteboard (pbcopy/pbpaste)
    axiomata-cli/                   # headless binary that exercises axiomata-core end to end
    axiomata-terminal/              # standalone PTY + VT100 engine for the Terminal module
    axiomata-board/                 # standalone Kanban core (M7.0), ships migration 8
    axiomata-ide/                   # standalone agentic-IDE core (M7.1/M7.2), migrations 9+10
    axiomata-files/                 # standalone file service of the file app / editor (ED0)
    axiomata-roster/                # engines + agent roles (AGENT.md), a2a.md CP-A1; ships migration 14 via axiomata-ide
  apps/
    axiomata/
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
- `agents` — agent backend dispatch (Opencode — the single harness — / the plain Ollama
  completion), opencode-based chat turns, the module bridge.
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

A boundary crate for macOS-specific integration beyond what MCP servers (Apple Mail /
Reminders / Calendar, used today via opencode's own MCP tool-calling — see §5 "Connector
modules") already cover. Its one module so far is `clipboard` (editor ED3.2, V3): the
general pasteboard as text through `/usr/bin/pbpaste` and `/usr/bin/pbcopy` (absolute paths,
`LC_CTYPE=UTF-8`, 16 MiB each way, killed after a 5 s deadline), behind the
`clipboard_read`/`clipboard_write` Tauri commands that Vi's `"`/`"+` registers use.

### `axiomata-terminal`

A standalone PTY + terminal-emulation engine backing the dashboard's Terminal module: no
dependency on Tauri or `axiomata-core`, so it is independently unit-testable and even runnable
on its own (`cargo run -p axiomata-terminal --bin term-poc`). `PtySession` (`portable-pty`)
owns the shell process; `Terminal`/`Screen` (`vte` for tokenizing, a hand-written
`vte::Perform` for everything the tokens actually *do*) is the cell-grid state machine —
cursor, SGR colours/attributes, scrollback, the alternate screen (`vim`/`less`/`htop`),
bracketed paste, and the visual bell. `apps/axiomata/src-tauri/src/terminal.rs` is the thin
Tauri glue (session registry + `terminal_spawn`/`_write`/`_resize`/`_close`/`_scrollback`
commands, streaming interpreted `Cell` snapshots — not raw bytes — over a Tauri `Channel`);
`apps/axiomata/src/modules/terminal.svelte` + `TerminalScreen.ts` render it on a `<canvas>`.
Full phased build log and the current checkpoint: `docs/plans/terminal.md`.

### `axiomata-board`

The Kanban board core backing the dashboard's Kanban module (milestone M7.0):
domain types, every SQL statement, and the initial schema as `SCHEMA_SQL_V1`. Like
`axiomata-terminal` it depends on neither Tauri nor `axiomata-core` — and here that is
load-bearing rather than tidy, because the agentic IDE (`axiomata-ide`, M7.1 onwards) will
use the same board as its agent task board and must not pull core in. It owns neither the
database file nor the connection: every operation is a free function taking a `&Connection`,
the way `routines::store` works. `axiomata-core` supplies the connection, ships the schema as
migration 8, and re-exports the crate as `axiomata_core::board`.

Two design points worth knowing before touching it. **The column carries the status, the card
does not** — a card's status is `board_columns.maps_to_status` of the column it sits in, so
"card says Done, column says Doing" cannot be represented at all, and re-pointing a column
re-states every card in it for free. **Verification needs a second party**: `verified_by <>
claimed_by` is enforced in the `WHERE` clause of `verify_card` (so a lost race is a clean
`false`) *and* as a `CHECK` constraint (so no future caller can route around the store), and
actor strings are canonicalised before either sees them — without that, `agent:one` could
claim a card and `Agent:One` could sign it off. Every mutation is a single statement with its
precondition in the `WHERE` clause; claiming is a compare-and-swap.

**The agent flow (A2A CP-A2, 2026-10-04, `docs/plans/a2a.md` A12–A19).** Schema version 2
(`SCHEMA_SQL_V2`, `flow.sql`, migration 15) and `flow.rs` give the board what agents need
without touching the principle above. A column may have a **role** (`stage`: `proposal` is
open, `review` is doing) that refines its status instead of adding statuses — the status
`CHECK` of version 1 stays, and **every board has both columns** (new boards start with
Vorschlag · Offen · In Arbeit · Review · Fertig, `ensure_flow_columns_all` adds them to older
boards at start; they cannot be deleted, the Vorschlag column is hidden by the views while
empty). A card's flow **state** (`TaskState`: proposed, blocked, ready, working,
input_required, in_review, done, verified, taken_over, failed, canceled) is *derived* on every
read from the column, the signatures and a few fields (`derive_state`), never stored. New
tables: `plans` (the unit of approval, automation and limits; `cards.plan_id`), `card_deps`
("needs first"; same plan, acyclic — checked in the transaction that inserts the edge —, ready
means the predecessors are **signed off**), `card_events` (append-only history: review notes,
escalations, limit stops). Steps for an agent are `report_done` (claimed card → review
column) and `review_verdict` (approve = move to Done *and* sign in one transaction; return =
back to work, `returned_count` + 1, a reason is required); nobody judges a card they hold.
`move_card_as` claims an unclaimed card for whoever hands it into the review column, so a
card the owner did alone can still be judged by an agent without loosening the two-party
`CHECK`. Plan approval (`approve_plan`) moves the plan's proposals to Offen — proposals and
the owner's yes are one concept. CLI: `board plan|dep|report|verdict|events|note|input|fail|
cancel|reopen|taken-over|approve`; the Kanban module shows the state badge, the agent fields,
dependencies and the history (`modules/kanban.svelte`, `core/kanban.ts`).

**Who may act is enforced, not believed** (a2a.md A39, from the CP-A2 security review). The store asks for a
`human:` actor at the owner's gates (approve plan/proposal, take-over, reopen), lets only the holder or the
owner set a question or fail/cancel a card, and refuses free moves by an `agent:` actor. The CLI derives the
actor in an agent session from `AXIOMATA_AGENT_ID/_NAME` (`axiomata_core::session`), refuses a different
`--actor`, closes the owner's commands there and lets an agent add cards only to the proposal column. This guards
against mistakes and against agents that follow their instructions; it is not a sandbox (a process of the same user
can clear the variables or write the database), which is CP-A5's job. Bounds: 2000 cards per board, 100 plans, 50
dependencies per card; titles and labels are one line.

**Kanban is an app, not a tile** (owner, 2026-10-04). The ring's *Kanban* entry (`view:kanban`, event
`shell:kanban`) opens the board as the large panel on the board used last (`modules/kanbanApp.ts`); the module `kanban`
is `stageOnly`, so no tile can be placed and a saved one is dropped when the dashboard loads (`RETIRED_TILE_TYPES`; the old
ring entry `kanban` migrates to `view:kanban`). Board management and card display moved from the tile's flip side into the
panel's gear popover (`modules/KanbanBoards.svelte`). The assistant's actions (`kanban_list_cards`, `kanban_add_card`,
`kanban_move_card`) are **shell actions**: the agent manifest lists mounted instances, and an app has none, so they have to
exist whether or not the panel is open.

`core/board_mirror.rs` writes each board to `<workspace>/Kanban/<id>-<name>.md` after every
change, one way only — see the trap list in `AGENTS.md`. Full plan and the list of what came
out differently in practice: `docs/plans/kanban.md`.

### `axiomata-ide`

The agentic IDE's core (milestone M7.1). Cut exactly like `axiomata-board` and for the same
reason: the IDE is meant to be extractable into a standalone app, so it depends on neither
Tauri nor `axiomata-core`, owns neither the database file nor the connection, and ships its
initial schema as a frozen `SCHEMA_SQL_V1` (migration 9, re-exported as
`axiomata_core::ide`).

Today it holds **projects** — a name, a folder, and the dock layout the user left behind in
it. Three things are load-bearing. `repo_root` is `UNIQUE` and canonicalised before it is
stored, so `~/x`, `./x` and `/Users/me/x` cannot become three projects fighting over the same
worktrees from M7.2 on. `layout_json` is **opaque to Rust** — the dock tree belongs to
`apps/axiomata/src/ide/layout.ts`, the way a tile's config belongs to the frontend in
`dashboard.json` — and `NULL` means "never opened", which is what triggers the starting
layout. And **deleting a project removes a row, never a folder**: the folder is the user's,
the row is ours, which is why the UI calls it "remove from the list".

One promise here is per module rather than crate-wide: the projects store looks at the file
system but never changes it. That will *not* hold for the worktree module in M7.2, which has
to create and remove real directories — it states its own contract when it lands.

### `axiomata-roster`

The Studio's **engines and roles** (`docs/plans/a2a.md`, A5, CP-A1) — pure like `axiomata-tasks`: no Tauri, no database,
no `axiomata-core`, it takes directories and returns data. Three levels, kept apart on purpose: an **engine** is harness +
model + environment (the owner's catalog, `config.agents.engines`, global so a switch happens in one place), a **role** is
a kind of work with a tier, an engine, limits and instructions (one `~/.axiomata/agents/<name>/AGENT.md`, YAML
frontmatter with `deny_unknown_fields` plus a Markdown body), and a **session** is a started agent with its worktree — still
the `ide_agents` row, which now carries `engine_id` and `agent_role` (migration 14, `axiomata_ide::SCHEMA_SQL_V6`).

Load-bearing: a role names engines **by id and never carries a command line**, so a role file arriving with a cloned
repository can change what an agent is told but not which program runs. Even so, a project's own roles
(`<project>/.axiomata/agents/`, `overrides.rs`) apply only after the owner confirmed their exact content: SHA-256 over every
file read, kept in `~/.axiomata/agent-roles-trust.json` (the store of `axiomata-tasks`, keyed by the overrides directory so it
cannot collide with `tasks.json`), and `confirm` re-reads the files and refuses if they differ from what was shown. A project's
files are read the moment the project opens, before any confirmation, so reading is hardened: role files open with
`O_NOFOLLOW` (symlinked directories/files are never followed), at most 64 directories / 1 MiB in total are read (more makes the
project *blocked*, i.e. not confirmable), the frontmatter is size- and nesting-capped and **anchors, aliases and tags are
refused before the YAML parser runs** (it expands aliases without limit — an alias bomb), control and bidi-formatting characters
are refused in role text, and the hash is over fixed-width digests of each name and content so no choice of names can collide.
A role name is a lower-case slug because it becomes a directory on a case-insensitive file system. `Harness` is defined here and re-exported by `axiomata-ide`.

`axiomata-core::roster` adds what needs config and database: `sync_agents` derives an engine from every agent profile that has
none (equal profiles share one; idempotent; at start and after an agent is created or edited), `save_engine`/`delete_engine`
(refused while a session or a role uses it; every change is applied to the freshly read file config under one lock — rows
included — and only `agents.engines` is written, never the live config, which would undo a workspace change queued for the next
start; a derived engine that does not validate leaves its agent unassigned instead of poisoning the config), and the role operations. The old
`harness`/`command`/`model`/`env` columns stay the fallback until CP-A6 moves starting over to the engine. The webview names a
project by id, never by path (`project_roles`, `confirm_project_roles`).

### `axiomata-opencode`

The client for Opencode 2's shared background service (`docs/plans/opencode2.md`, OC1).
Standalone like `axiomata-terminal`: no Tauri, no `axiomata-core` — the skill runner (in
core) and the agentic IDE (`axiomata-ide`, OC2/OC3) both use it. `service` finds the
service (`opencode debug paths state` → `service.json`), logs in (the one undocumented
detail: HTTP Basic, user `opencode`), refuses a non-loopback URL and a major version other
than 2, and starts the service when it is not running. `events` reads `GET /api/event`
(server-sent events; volatile by contract). `session` creates, prompts, reads (backwards,
page by page, up to a turn's prompt), steers and deletes sessions. `turn` puts them together
into one unattended turn (see "Agent backends" below). The event payloads are not in
Opencode's OpenAPI document; the ones used were measured against 2.0.18 and are listed in
the plan. `tests/live.rs` holds `#[ignore]`d checks against the real service with a local
model.

### `axiomata-files`

The file service behind the file app and its own editor (`docs/plans/editor.md`, milestone
ED0, decisions E1–E12). Standalone like `axiomata-terminal`: no Tauri, no `axiomata-core`.
Everything a webview may touch on disk goes through a **`Root`** — a canonical directory (or,
for one picked file, that file) plus a **link policy**: `Strict` (no symlink, no hard link —
the Second-Brain workspace, which agents write into unattended) or `Contained` (a symlink is
followed only if its target stays in the root; hard links allowed, because pnpm's
`node_modules` is made of them). A file is always named as *root id + relative path*, never as
an absolute path the webview could make up.

Four things are load-bearing:

- **The guard is re-checked at the moment of use** (`pinned.rs`). `Root::resolve` proves a
  path safe once; every read, write and delete then walks down from the root's directory fd
  with `openat(O_NOFOLLOW)` and acts relative to the parent fd it holds, so a directory
  swapped for a symlink in between cannot redirect it (the ED0.1 security audit's TOCTOU
  finding; `rustix`, no own `unsafe`). The target is re-checked on the open fd too (regular
  file; no hard link under `Strict`; `O_NONBLOCK` against a swapped-in FIFO).
- **Content versions, not mtimes** (`file.rs`). Every read returns `<len>-<FNV-1a>`; a write
  can demand it and fails with `Conflict` otherwise — the basis of "an agent changed the file
  you have open". FNV-1a is pinned by a test because versions will be persisted (recovery).
- **Grants** (`grants.rs`, `~/.axiomata/file-grants.json`, `0600`) are the only way a new
  place becomes reachable: a file or folder picked in the native dialog, which is driven from
  Rust — the dialog plugin's JS API is deliberately not in `capabilities/default.json`.
- **The watcher judges by content** (`watch.rs`, `notify`/FSEvents). It watches the parent
  directory (an atomic rename-over would silence a watch on the file), debounces 150 ms, and
  reports `modified`/`deleted`/`created` only when the version really changed; reads and
  hashes run outside its state lock.

Which roots exist is the embedder's business, answered through the `RootResolver` trait:
`axiomata_core::files::Roots` resolves `workspace` | `project:<id>` | `worktree:<agent id>` |
`grant:<id>` freshly on every call from config, SQLite and the grant file, so a moved
project, a discarded worktree or a revoked grant is noticed at the next access. The old
`*_workspace_file` commands keep their names, 1 MiB cap and string errors but run through
this crate (`core::workspace` delegates); the new `file_*` commands use the editor's limits
(read 16 MiB, `large` above 2 MiB, write 2 MiB) and typed `{ kind, message }` errors.

**Language servers (ED6, `lsp/`).** `axiomata-files::lsp` runs the editor's language servers
(`docs/plans/editor.md`, L1–L4, L10). `servers.rs` is the only place that decides which
program runs: a built-in table (one server id per server, TypeScript/JavaScript/TSX sharing
one), found on `PATH` plus `/opt/homebrew/bin`, `/usr/local/bin`, `~/.cargo/bin`,
`~/.local/bin` (a Finder-started app has no shell `PATH`), overridden or switched off only in
`~/.axiomata/lsp.json`, which no Tauri command writes — the webview names a root and a
language, never a program (L2). `LspHost` runs one server per (root id, server id) in the
root's folder, frames messages (`framing.rs`, `Content-Length`, ≤ 64 MiB) and passes them
through whole; stderr is discarded. A start names the page asking: the same page gets the
running server back, a reloaded page restarts it (a server refuses a second `initialize`).
The embedder counts open documents (`opened`/`closed`); a janitor thread stops a server ten
minutes after its last one closed, sending `shutdown`/`exit` and killing it after 2 s. When a
server's output ends the page gets `$/axiomata/exited`. Single-file roots get none; the
Tauri glue (`src-tauri/src/lsp.rs`: `lsp_start` with a `Channel`, `lsp_send`, `lsp_opened`,
`lsp_closed`) also refuses every `grant:` root. What the page may send is limited in Rust,
not in the page (ED6.1 security review): only `ALLOWED_METHODS` — the handshake, document
sync, `$/cancelRequest` — and answers to the server's own requests pass, so a compromised
webview cannot make a server run its commands (`workspace/executeCommand`); the list grows
with each checkpoint's requests. Every message, open and close must carry the page token of
the server's starter; outgoing messages are capped like incoming ones and written through a
per-server `stdin` lock, never the host's; at most `MAX_SERVERS` (8) run, and another page
restarts one at most every 2 s. ED6.2 adds `textDocument/hover` and `/definition` to the list (ED6.3: `/implementation`,
`/typeDefinition`, `/references` — every request of `LOCATION_METHODS` is tracked like a definition;
ED6.4: `/completion` and `completionItem/resolve` — their answers free nothing, and an item's `command`
is never run; ED6.5: `/formatting`, `/prepareRename`, `/rename` — a rename's files are written by the page
through the file service, never by this host)
and makes one answer readable in Rust (L11): the ids of definition requests are noted, their
answers are parsed in the pump thread, and the files they name (`uri`/`targetUri`, at most
4096 per server) become readable through `LspHost::read_foreign` — but only when they lie under
the server's **toolchain folders** (`servers::toolchain_roots`: the program's own install root —
its Homebrew keg or `node_modules` — plus per server `rustc --print sysroot`, `~/.cargo/registry/src`,
`~/.cargo/git/checkouts`, `~/.rustup/toolchains`; Python's prefixes; Xcode and the Command Line
Tools). Without that limit a project file could make a server name any file (`#[path = "…"]`, a
reference) and the page read it (ED6.2 security review, CRITICAL); a root at or above `$HOME` (or `/`) is dropped. The read opens the path with
`O_NOFOLLOW`, checks the open fd is a regular file whose path (`F_GETPATH`) is the named one, and
reads from that fd — no check-then-open race; ≤ 16 MiB, UTF-8. At most 256 definition requests
may wait for an answer per server. `file_read` on `lsp:` does not check the page token: the token
is self-asserted by the page, and the toolchain allow-list is the boundary. The file app opens
such a file under the read-only root `lsp:<handle>` with the absolute path as `rel`:
`file_read` routes it to `read_foreign`, `file_watch` does nothing for it, a write fails as
for any unknown root. `tests/lsp_live.rs` checks real `rust-analyzer` (diagnostics, hover,
a definition into the Rust standard library read back) and `pyright` (`#[ignore]`d).

### The editor (`apps/axiomata/src/editor/` + `src/fileapp/`, ED1)

The file app's own editor, split along the D1 line: **`src/editor/` is the engine** —
plain TypeScript with no DOM, no Svelte and no imports from the rest of the app, tested
with vitest (`buffer` behind the `TextStore` interface, which a rope replaces in ED5 on one
constructor line; `document` with linear undo and step grouping; `commands` + `keymap`
for the Mac key map; `wrap`/`visual`/`geometry` for soft wrap, visual rows and every pixel
rule in cells). **`src/fileapp/` is the app around it**: `EditorSurface.svelte` renders
only the visible rows and takes input through a hidden textarea (IME, dead keys, the
native clipboard events); `FileAppView.svelte` is the full-screen view (hidden, never
unmounted, like the IDE); `session.ts` holds every file flow — expected-version saves,
external changes judged by version, recovery — behind a `FileBackend` interface and is
tested with a fake one. Three load-bearing facts: geometry is arithmetic because every
bundled font is monospace (one measured cell width; wide characters take two cells); word
boundaries use their own character classes, not `Intl.Segmenter` (which treats `bar.baz`
as one word); and `EditorDocument` is a mutable class, so the surface redraws on its own
`tick` after every `doc.*` call and on the `revision` prop for changes made behind its back.
Unsaved text is kept in `~/.axiomata/editor-recovery/` (`core::editor_recovery`, at most
256 entries), preferences in `editor-settings.json` (`core::editor_settings`), what Vi
remembers (named registers, file marks, histories) in `editor-vi.json` (`core::editor_vi`,
schema owned by `fileapp/viPersist.ts`), recent files under `settings.editor.recent` in
`dashboard.json`.

**Syntax (ED2)** lives in `src/editor/syntax/`: tree-sitter via `web-tree-sitter` (the only
library the engine uses — for a sub-problem, like `vte` for the terminal), grammars built by
`apps/axiomata/scripts/build-grammars.sh` from pinned tags and checked in under
`public/grammars/`, loaded on first use through an injected `GrammarSource`. The tree follows
every edit incrementally via `EditorDocument.onTextChange`; only visible lines are queried;
injected languages (Markdown fences, Svelte `<script>`) are parsed apart and cached by text.
Two traps: never read `node.text` (web-tree-sitter re-calls the parse callback with a stale
position — slice the store), and the CSP carries `'wasm-unsafe-eval'` for tree-sitter. Colours
are 17 `--ax-syntax-*` plus six `--ax-editor-*` tokens per theme; the Markdown preview reuses
`core/markdown.ts` (`renderMarkdownBlocks`, `data-line` per block) and `core/markdown-prose.css`,
shared with the chat.

### `axiomata-cli`

A `clap`-based binary whose job is to exercise `axiomata-core` end to end without the GUI:
`status`, `list-skills`, `run-skill`, `list-runs`, `memory sync|status`,
`routines list|add|edit|delete|enable|disable|history|tick`,
`board list|new|rename|delete|add|move|claim|done|verify|archive`,
`ide projects list|new|rename|set-root|delete`, `files roots|read|write|grants`, `assistant` (one chat/instruct
turn, `--allowed-tools` kept for API symmetry — the opencode harness auto-approves tool use),
`import obsidian`, `graph`, `modules`, `module-action`. Run it with
`cargo run -p axiomata-cli -- <subcommand>`.

### `apps/axiomata/src-tauri`

The Tauri shell, Cargo package name `Axiomata-OS` (matches what the macOS menu bar shows
during `cargo tauri dev`, since a dev run has no bundled `.app`/`Info.plist`; the `[lib]`
target stays `axiomata_lib`). Depends on `axiomata-core` via a path dependency. Its
`.setup()` hook (`bootstrap.rs`) calls `AxiomataCore::init()`, kicks off a best-effort memory
sync on a background thread, starts the routine scheduler
(`tauri::async_runtime::spawn(routines::serve(…))`, stop handle managed), and stores the
`AxiomataCore` as managed state. `AxiomataCore` holds `config` unlocked and only `db` behind a
`Mutex`, wrapped in an `Arc` so the scheduler task can hold its own handle. Plugins:
`tauri-plugin-opener`, `tauri-plugin-window-state` (the window remembers its geometry
across restarts) and `tauri-plugin-dialog` (the file app's open dialog — called from Rust in
`files.rs` only, none of its JS permissions granted). `files.rs` holds the `file_*` commands
and the managed `FileWatch`; `lib.rs`'s `on_page_load` drops every file subscription when the
page reloads. See §5 for the full command surface and the Svelte frontend.

### `apps/axiomata/src` (frontend)

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
- `terminal-settings.json` — the Terminal module's own global preferences (font, theme,
  shell, cwd, env, scrollback size, cursor, bell, opacity), shared by every placed Terminal
  tile rather than living in `dashboard.json`'s per-instance config (Checkpoint 5d of
  `docs/plans/terminal.md` — closing/removing a tile used to discard its settings along with
  it). Same read/write/recovery contract as `dashboard.json`, both backed by the shared
  `crate::json_state` machinery.
- `module-context.md` / `module-actions/{inbox,outbox}/` — the agent → module bridge (§5).
- `memory-last-sync.json` — the memory router's per-workspace staleness marker.
- `editor-settings.json` / `editor-recovery/` — the editor's preferences and its kept
  unsaved text (one entry per file, at most 256, swept after 30 days).
- `editor-vi.json` — Vi's named registers (so macros), file marks `A`–`Z`, command and
  search histories and last search (0600; a register over 256 KiB is not written).
- `agents/<name>/AGENT.md` — the Studio's agent roles (`axiomata-roster`, a2a.md CP-A1); the `allrounder` role is seeded if
  absent. `agent-roles-trust.json` holds the owner's confirmations of project role overrides (by content hash).
- `file-grants.json` — files and folders picked in the file app's open dialog, the only
  places outside a registered root the file service may touch (`axiomata-files`, §3).

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

Execution dispatches through a small `enum`, `AgentBackend { Opencode, Ollama { model } }` —
deliberately not a trait/registry (see §6). `AgentRequest` carries `prompt`, `cwd`, `timeout`,
`system_prompt_file` (the module bridge manifest, appended to **chat turns only** —
skill/routine runs omit it), `model`, and `allowed_tools` (declarative only; see below).

- `opencode.rs` is the single agent harness. Since OC1 (`docs/plans/opencode2.md`,
  2026-09-27) it is a **client of Opencode 2's shared background service** through the
  `axiomata-opencode` crate, not a spawner of `opencode run`: a skill run is a fresh session
  (`POST /api/session` with `location.directory = <workspace_root>`, the `provider/model` as
  a `Model.Ref`, a title, and permission rules), an assistant turn a fresh or continued one
  (switching the model first when the chat provider changed). `Service::run_turn` opens the
  event stream, sends the prompt, waits for `session.execution.succeeded|failed|interrupted`,
  and then reads the turn's messages back — reply text, tokens, cost, turns, `finish` and the
  `idle` entry's outcome all come from there, so an Opencode update can no longer change the
  result format underneath (the 2.0.17 → 2.0.18 stream change failed every run until
  `1b6dbfb`). Unattended permission rules (`unattended_permissions`): with
  `auto_approve_tools` everything not explicitly denied is allowed (the former `--auto`),
  without it every `permission.asked` is rejected — which interrupts the turn — and the
  `question` tool is always denied, so a turn never waits for an answer nobody gives. A bad
  `cwd` fails before the service is asked (same `ENOENT`/`ENOTDIR` as the old spawn), and
  under `cfg(test)` the backend never connects, so no unit test can reach the owner's real,
  possibly billed service. Discovery is `opencode debug paths state` → `service.json`
  (`url`, `password`); the service is started with `opencode service start` when it is not
  running, and a major version other than 2 is refused. The login (HTTP Basic, user
  `opencode`) is the one undocumented piece, kept in `axiomata-opencode::service` alone
  (upstream issue anomalyco/opencode#51724). The prompt reaches whichever provider
  `skill_provider` (skills) / `chat_provider` (assistant bar) routes to using that
  provider's own native tool-calling protocol — so a non-Anthropic model (Deepseek via
  OpenRouter, or a local Ollama model) executes its skills the way it does inside opencode
  itself. The old `claude -p` Anthropic framing was the root cause of the digests' off-topic
  prose / empty-results / 10-minute-timeout runs, and was retired along with the Stufe 2
  `ollama-agent` tool loop: one harness for everything. The service uses the user's opencode
  config (`~/.config/opencode`, MCP servers incl. `apple-mail`/`apple-reminders`, credential
  store); Axiomata passes no provider env. The model id is built by `opencode::model_id` /
  `opencode::chat_model_id` as `provider/<model>` from the role's provider + its model (or a
  skill's `model:`). `allowed_tools` frontmatter is documentation only — there is no runtime
  allow-list (that was the Claude Code `--allowedTools` mechanism, which silently refused MCP
  calls headless and is gone with it).
- `ollama.rs` makes one non-streaming `POST /api/generate` call to the local daemon — the
  simple, tool-free `backend: ollama` completion for deterministic tasks.

### Model providers (`config.agents.providers`)

Orthogonal to the `AgentBackend` `enum` above: a **provider** selects *which upstream* the
session's model points at (`openrouter/deepseek/…`, `anthropic/claude-haiku-4-5`,
`ollama/qwen3.8:27b-mlx`). Opencode resolves the provider's auth/keys itself from its own
credential store, so Axiomata carries no `ANTHROPIC_*` env plumbing. Not to be confused with
`AgentBackend::Ollama` (`agents/ollama.rs`), the separate raw/tool-free completion backend
selected per skill by `SKILL.md`'s `backend: ollama`.

- `config.rs` defines `ProviderId { Anthropic, OpenRouter, Ollama }` (`ProviderId::ALL` is the
  single source of the list — loop over it, never enumerate the variants by hand) and, per
  provider, `ProviderSettings { base_url, api_key, chat_model, skill_model }`, all kept under
  `agents.providers: BTreeMap<ProviderId, ProviderSettings>`. Every provider's fields are
  retained even while unused, so switching in the Settings dialog never discards what was typed
  for the others.
- **Per-role provider.** The provider is chosen *per role*, not globally:
  `agents.chat_provider` and `agents.skill_provider` (each a `ProviderId`), resolved through
  `AgentDefaults::provider_for(ProviderRole::{Chat,Skill})`. Interactive dashboard chat routes
  through `chat_provider`; every skill / routine run through `skill_provider`. So Anthropic (or
  OpenRouter) for chat while Ollama serves the connector digests is a supported config. The old
  single `agents.active_provider` key is migration-only (see **Migration** below).
- **Model selection.** `agents::default_chat_model()` reads
  `providers[chat_provider].chat_model`, `default_skill_model()` reads
  `providers[skill_provider].skill_model` (shared `provider_model()` helper), both feeding the
  `provider/<model>` model of the Opencode session. `chat()` uses the chat model; the skills runner uses
  the skill model as its fallback when a `SKILL.md` has no own `model:` frontmatter (per-skill
  frontmatter still wins). No provider-specific model env var anywhere — both non-Anthropic
  providers route
  on the `model` field in the request body.
- **Spend.** `spend::guard_redirected_turn(db, config, role)` checks the daily cap against the
  spend of *that role's* provider; `spend::role_spend_summaries()` returns one `SpendSummary`
  per distinct provider across the two roles (a single `"chat & skill"` entry when they match).
  The per-run `provider` column already carried the role's provider token, so the CP4/CP5
  rollup needed no schema change. The guard only ever bites a paid provider over its cap — a
  local `skill_provider = ollama` records ~0 spend and always passes.
- **Metered cost (CP5).** The daily cap meters recorded spend from token counts × an
  owner-configured per-model price table, `config.agents.costs` (`ModelCost` — USD per million
  input/output tokens, keyed by the *bare* model id, no `provider/` prefix): the cost opencode
  reports for a non-Anthropic model can be an order of magnitude too high, which
  made the cap fire on spend never incurred. `spend::metered_cost_usd` computes the figure and
  the runner substitutes it for the recorded `cost_usd` when the model is priced. The `model`
  column on `runs` (migration 0007) records which model a run used, and
  `spend::reconcile_recorded_costs` re-meters already-recorded runs/chat turns against the
  price table on every startup (best-effort; a failure is warned about, never fatal).
- **Runtime mutation.** The settings dialog is the first thing that writes `Config` at
  runtime, so `AxiomataCore.config` is `Arc<RwLock<Config>>` (many reads, rare writes). Every
  read site clones the `Config` out from under the lock in its own statement (`read_config()`
  in `commands.rs`, mirrored in `axiomata-cli`), never holding the guard across an `.await`;
  the routine scheduler is handed the *same* `Arc` (not a startup value-clone) and re-reads a
  fresh snapshot before each `tick()`, so provider/model changes apply to routine firings live
  too. `get_config` returns the full editable config; `save_config` (pure `apply_config_update()`
  helper, unit-tested without a Tauri harness) validates, writes via `Config::save()` (which
  keeps the file `0o600` — it can now hold an OpenRouter key), and swaps the in-memory copy.
- **Workspace root is the exception.** A live workspace swap has too wide a blast radius
  (memory router, particle graph, module manifests, every open note assume it is fixed for the
  process lifetime), so a changed `workspace_root` is written to disk immediately but *not*
  applied in memory — `save_config` returns `true` so the UI can prompt for a restart. Every
  other field (owner, providers, models) applies live, effective on the next agent turn.
- **Migration.** Two on-load upgrades run in sequence: (1)
  `migrate_legacy_model_if_needed()` seeds every `ProviderId::ALL` member with its defaults and
  folds a flat pre-`providers` `agents.claude_model` into Anthropic's model fields; (2)
  `migrate_legacy_provider_if_needed()` folds a pre-split `agents.active_provider` into **both**
  `chat_provider` and `skill_provider` (unconditional — a config carrying that key is by
  definition pre-split), and `Config::save()` then drops the legacy key
  (`#[serde(skip_serializing)]` on `legacy_active_provider`). Both are no-ops on an
  already-current config.
- **Settings UI.** `shell/Settings.svelte` has a **Vault** section (editable path + restart
  prompt) and a **Modell-Provider** section: two `<select>`s map `chat_provider` /
  `skill_provider` to a provider, and the provider list below is an *edit* selector
  (`editingProvider`) — clicking a row opens that provider's form (base URL + masked key,
  hidden for Anthropic, plus the two model fields); `[Chat]` / `[Skills]` badges mark the
  role providers. `providerLooksSane()` mirrors the Rust per-role validation loop. Persist
  through `get_config`/`save_config` (TS shapes in `core/backend.ts`, browser-mode fixtures in
  `core/devmock.ts`). LM Studio is still a one-line `ProviderId` addition. Full rationale:
  `docs/plans/settings-provider-overhaul.md`, `docs/plans/per-role-provider.md`.

### The `prepend_files` frontmatter

One `SKILL.md` frontmatter mechanism surviving from Stufe 2's lean-local-agent plan
(`docs/plans/stufe2-lean-ollama-agent.md`); the plan's other mechanism (`local_backend`, the
`ollama-agent` tool loop) was retired in Stufe 2 CP5 — every skill now runs on
`AgentBackend::Opencode` regardless of provider.

- **`prepend_files: ["Mail/.topics.md"]`** — workspace files read at run time and prefixed to
  the prompt as `## Context file: <rel>` blocks (missing/blank files contribute nothing; a
  `..`/absolute entry is rejected). The generic fix for a skill that references a workspace
  file but runs on a backend with no file tool — the opencode harness doesn't get `mail-digest`'s
  topics file as context, so `mail-digest` needs its configured topics inline. Applies to
  **every** backend. Applies live to the routine scheduler's `skill` targets too (they go
  through `execute_skill`); a raw-`prompt` target has no `SKILL.md`, so no `prepend_files`.

### The `output: json` frontmatter (2026-09-29)

The three connector digests (`mail-`, `calendar-`, `reminders-digest`) promise one JSON object that
their dashboard module parses. A small model sometimes ends on prose *about* the JSON instead of the
JSON (run 988, `ling-3.0-flash`: ~3 % of the mail runs since the Opencode move). `output: json` (or
the default `text`; anything else makes the skill invalid) is the contract: when the reply of an
`opencode` run holds no JSON object, `agents::opencode::run` sends **one** repair prompt into the
same session (`JSON_REPAIR_PROMPT`, at most 180 s on top of the skill's own limit) and folds its tokens
and cost into the run (a repair that times out is paid for but not recorded). The check
(`find_json_object`) follows the dashboard's `firstJsonObject` (`core/skillRun.ts`): the first
balanced `{…}` that parses; a balanced `{…}` of prose is stepped over, a truncated object is none
(where they differ, Rust errs towards repairing).
Ollama ignores the field. On the dashboard side `loadLatestSkillRun` reports the newer run it passed
over (`skipped`), and the Mail tile says so (`staleDigestNote`) instead of quietly showing an older
digest. Bundled skills are seed-if-absent: an edited `SKILL.md` reaches an install through
`skills reseed --force`.

### Skills runner (`skills/`)

Skills live in **one** place: `~/.axiomata/skills/<name>/SKILL.md`
(`registry::list_skills`/`find_skill`, frontmatter via `gray_matter`, filesystem is the only
source of truth). `runner::execute_skill` resolves the backend (`opencode` by default; `ollama`
for a tool-free local completion), builds the prompt (any `prepend_files` blocks, then the
`SKILL.md` body) and runs it without touching the database; `execute_and_record_skill` adds the
DB/log write. `runlog.rs` persists to the SQLite `runs` table and appends JSONL to
`logs/runs.log`. A connector skill's SOP reaches its MCP tools through opencode's own config —
`allowed_tools:` frontmatter is documentation only (see "Agent backends" above).

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
`axiomata-cli routines tick` runs — and that is now the intended end state, not a stopgap:
always-on scheduling (M4) was dropped outright (§6).

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
  answers in `outbox/`; the CLI exits 2 on timeout. Appended to **interactive chat turns
  only** (`agents::chat`); skill and routine runs omit it — nothing they do needs a module
  action and it was ~1.6K tokens resent on every step of the agent loop (Stufe 1, 2026-09).
  A future skill that needs module access should reintroduce it behind a `SKILL.md`
  frontmatter flag.
- **Themes**: `<html data-theme="…">`, every colour/size through a `--ax-*` token (graphite,
  paper, steampunk, forest, ocean); a user `~/.axiomata/theme.css` is validated
  (`:root { --ax-*: … }` only) before injection.
- **UI scale** (`docs/plans/editor-look.md`, LK0): `--ax-ui-scale` on `<html>` multiplies every UI
  size — type (12/13/14/16/20 px), spacing, radii, icons (`--ax-icon-*`), hit targets
  (`--ax-hit-min`), panel and dialog widths. "Auto" (`core/uiScale.ts`) asks Rust (`ui_displays`,
  `axiomata-macos::display`, CoreGraphics FFI) for each display's points per inch against what macOS
  is designed for (127 built-in, 110 external), 1.0–1.6; it follows the window to another display.
  The "UI size" setting (Settings → Darstellung) overrides it. The canvas keeps tile geometry in
  unscaled units and draws it times the scale (tiles grow with their text instead of cutting it
  off); the floating panels and the file tree's width work the same way. The editor's and the
  terminal's fonts are content, not UI, and stay as set. UI icons are a vendored Lucide subset
  (`scripts/vendor-icons.sh` → `src/ui/icons/lucide.ts`) drawn by `ui/Icon.svelte`; clickable ones
  are `ui/IconButton.svelte` (at least `--ax-hit-min`, label required).
- **File app look** (`docs/plans/editor-look.md`, LK1–LK2): the right-hand inspector
  (`fileapp/Inspector.svelte`: Settings | Shortcuts; `fileapp/shortcuts.ts` is tested against the real key map and
  the Vi grammar) is a column beside the tabs, so the minimap stays in view. The tabs sit in groups side by side or
  stacked on the IDE's dock tree (`fileapp/fileDock.ts` over `ide/layout.ts`, drag geometry from `ide/dock.ts`,
  editors moved between groups by `ide/paneStore.ts` so a move keeps cursor, undo and unsaved text); a file is open
  at most once in the whole layout. LK3–LK5: tabs are pills tinted by language (`core/languageColors.ts`, tokens
  `--ax-lang-*`), text buttons are `ui/IconButton`s, the file tree has icons in four styles (`core/fileIcons.ts` over
  the sets vendored by `scripts/vendor-file-icons.sh` — Catppuccin, Octicons, JetBrains Expui — and Lucide for
  monochrome), and the cursor's glide lasts longer the farther it goes, with a smear at "strong"
  (`editor/decorations.ts` `glideMotion`/`stepCursor`, drawn by `fileapp/cursorGlide.ts`).
- **Connector modules — "provider = skill, not code"**: Calendar and Reminders (and Mail)
  are the pattern for any future integration behind an MCP server the app doesn't have
  first-class Tauri commands for. A `*-digest` skill (seeded on first run from
  `crates/axiomata-core/resources/<name>/SKILL.md`) reads the source via an MCP server's
  tools and replies with one JSON object; **there is no live poll** — data sits behind an
  MCP tool only an agent can reach, so every refresh is a real agent turn (whichever run
  happened most recently: by hand, on a schedule via a Routine, or the tile's own ↻, all the
  same `run_skill` mechanism). Since Stufe 2 CP5 a digest's refresh is an Opencode turn
  against whichever model `skill_provider` routes to — cloud (Deepseek via OpenRouter,
  Anthropic) or local (`ollama/<model>`) alike, no backend switch needed (§"Agent backends").
  The Calendar tile goes further on the client: a Monday-first
  **mini-month** (`core/monthGrid.ts` + `modules/MiniCalendar.svelte`) plus an agenda
  showing only the **selected day … +7** — so `calendar-digest` fetches a wide window (this
  month + next) once and every day-click / month-page is a free client-side filter, not a
  new run. An **optional clock** (digital or analog, `config.showClock` / `clockStyle`) sits
  beside the mini-month. `core/skillRun.ts`'s `loadLatestSkillRun` (find
  the most recent run of skill X) and its `stripCodeFence` (the model adds a ` ```json ` fence
  despite the SOP saying not to) are shared connector infrastructure. **Writes** (create /
  complete / delete) go through a fresh, silent one-shot **instruct turn**
  (`core/instruct.ts`'s `runInstructWrite`) instead: a skill's SOP is fixed at authoring
  time and has no way to take a form's runtime parameters, so the instruction spells out the
  exact MCP tool call in the instruction text itself and the caller updates its already-loaded
  digest locally (no full re-run) from the reply.
- **HTML pages** (courses): rendered via `<iframe sandbox="allow-scripts" srcdoc=…>`
  (`fileapp/HtmlPreview.svelte`, the editor's rendered view of an HTML file)
  — **not** an `asset://` URL. An earlier `asset://` + `<iframe src=…>` design looked correct
  (even reported `is_allowed == true` on the Rust side) but every lesson rendered a blank
  white frame — "403 (Forbidden)" / sandboxing refusal in the WebKit console, matching known
  issues with Tauri's asset/custom-protocol handling inside sandboxed iframes. `srcdoc`
  sidesteps the whole asset-protocol question, at the cost of `core/htmllink.withNavIntercept`
  handling same-folder links and same-page anchors by hand (a `srcdoc` document's *base URL*
  for resolving relative `href`s is the embedding app, not the lesson's real location, per the
  HTML living standard).
- **Modules shipped today**: memory-status, skills-deck, routines-board (§5 above), the file
  panel (`file`, panel-only since ED4: the editor opened from the Second Brain, the chat or an
  agent — Markdown and HTML rendered, code, images, "New note"), todo (a flat
  `ToDo.md` checklist, GFM task lists, inline-editable), calendar, reminders, mail (connector
  modules per the pattern above), terminal (a self-built PTY/VT100 emulator, `axiomata-terminal`
  — see §3), and second-brain (below) — each with a front and, where relevant, a settings face.

### Second Brain — particle graph

`axiomata_core::graph::build` (command `get_workspace_graph`) walks every tracked file
(reusing the memory walker) plus skills and routines as graph nodes, and every `[[wiki]]` /
relative Markdown link / relative HTML `href` as an edge, capped at 5000 files. The frontend
(`apps/axiomata/src/graph/`) renders this with Canvas 2D (no graph library) in three
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

**App Ring** (`core/apps.ts`, `graph/model.ts`'s `buildAppNodes`/`layoutAppRing`): a ring of
launcher nodes around the Orbit background widget, builtin modules on one side of the "+" and
externally added Mac apps on the other, plus owner-defined groups on either side
(`core/appGroups.ts`) that collapse to one ring-slot node and expand to a secondary ring on
click. Since Checkpoint 5c a builtin can be hidden from the ring (and un-hidden again) from
the "+" dialog's "Intern" tab — `hiddenBuiltins`, a `~/.axiomata/dashboard.json` store next to
`userApps`, opposite default (everything ring-eligible shows until explicitly hidden, vs.
`userApps` starting empty). A ring click on a singleton builtin (`ModuleDefinition.singleton`
true/unset) brings its placed instance to front or creates one; a non-singleton builtin (only
`terminal` today) always creates a fresh instance. Full design and checkpoint history:
`docs/plans/app-ring.md`.

## 6. What is designed but not yet implemented

### Why the agent backend is an `enum`, not a trait

Skill and routine execution dispatches through `AgentBackend` (an `enum`, today
`{ Opencode, Ollama }`), not a plugin registry or trait-object abstraction — a
deliberate choice, since only a few backends are needed and a generic multi-CLI abstraction
would be premature generalization. A further backend can gain a variant without reworking the
runner or scheduler.

### M4 — always-on / background scheduling: dropped

The app runs as an ordinary desktop app the user starts and quits; closing the window ends
the process, which also stops the routine scheduler. Autostart (`tauri-plugin-autostart`),
single-instance guarding (`tauri-plugin-single-instance`), close-to-hide window behaviour and
a true background scheduler were this phase — **cancelled outright by the owner on
2026-09-20**, not merely deferred. The reasoning: either the app is open, or the tiles
refresh when it is opened, and that is enough. Nothing should be designed around a future
always-on mode; the scheduler's "fires only while something runs" behaviour (§5) is the
intended end state.

### M7 — the agentic IDE

Planned in full in [`plans/agentic-ide.md`](plans/agentic-ide.md): an own full-screen IDE
view with a dock/split/tab layout, Claude Code and Opencode hosted as PTY tiles in the
existing terminal engine, agent-to-agent messaging over an own MCP server (rather than
reading the agents' screens), one git worktree per agent so diffs are separable, and an own
mini-harness for small, precisely executed tasks — built to be extractable into a standalone
app the way `axiomata-terminal` is.

**M7.0 (the Kanban board) is done** — see `axiomata-board` in §3 and
[`plans/kanban.md`](plans/kanban.md). It shipped first on purpose: the board is useful
without a single agent, and it is exactly the data layer the agents' task board sits on, so
building it afterwards would have meant designing it twice — once for humans and once
concurrency-safe.

Its final live-test round (CP-K4, `plans/kanban.md` §6b) also corrected three things in the
*shared* shell that had gone unnoticed and now apply to every module: a new tile opens
centred on the canvas and is nudged only when that exact spot is taken (`core/lifecycle.ts`),
a floating panel's remembered size hangs off a key per panel kind rather than one shared key
(`shell/StagingPanel.svelte`, which is why a card detail no longer inherits the big board's
size), and `openStaged`'s anchor lets a panel open over its own tile instead of the screen
centre.

**M7.1 is done** (CP0–CP3). Its first checkpoint (CP0) is the crate `axiomata-ide`, holding
IDE *projects* — a name, a folder, and the dock layout the user left behind in it — with
migration 9 and the CLI group `axiomata-cli ide projects …`. It is cut exactly like
`axiomata-board`: no dependency on `axiomata-core` or Tauri, no owned connection or path,
and a frozen `SCHEMA_SQL_V1` guarded by a checksum test. Migration 9 deliberately holds
**only** `projects`; agents, worktrees, plan steps and the mailbox arrive with their own
migrations in M7.2/M7.4, so a frozen schema carries no guesses. Two rules worth knowing
before touching it: `repo_root` is UNIQUE and canonicalised on the way in, and deleting a
project removes a row and **never** a folder.

**One project registry for editor and IDE** (2026-09-30, `docs/plans/editor-projekt-werkzeuge.md`): the file app's
tree shows exactly one open project (`settings.editor.tree.project`), the IDE keeps its own; both open folders through
the same Rust commands (`files.rs` `project_open` / `project_new`, which drive the native dialog — the webview never
types a path; `create_ide_project` and `set_ide_project_root` were removed for that reason; "change folder" is `project_set_root`). `store::open_root` finds or creates the row for a
folder, `newproject::create_project_folder` makes a new folder (optionally `git init`) and is the one place in
`axiomata-ide` that creates directories. No project file is written into the folder.

**Git layer** (2026-10-01, `docs/plans/editor-projekt-werkzeuge.md` #48): `crates/axiomata-git` is the one place that runs `git`
(subprocess, never `git2`; the only thing that publishes is `repo::push` — the checked-out branch to its upstream or `origin`, never forced, only on the owner's button) and reads its machine formats — `run` (the runner and its environment), `diff`
(diff types, parser, hunk patch), `repo` (status, stage/unstage, hunk apply, discard, blob, commit, fetch, push, branches, init). It has no Tauri,
database or macOS-only code, so it builds and tests anywhere `git` runs. `axiomata-ide::git` (agent worktrees) sits on
it through thin wrappers that keep the IDE's error type; the editor's git panel talks to it through `src-tauri/src/git.rs`.

**CP1 is done too**: `apps/axiomata/src/ide/layout.ts` is the dock-layout model — a tree of
`Split { dir, children, sizes }` and `TabGroup { tabs, active }` with docking, moving,
closing and divider dragging, plus the serialisation that fills CP0's `layout_json`. Pure
logic with its own vitest file and no DOM, the same cut as `core/kanban.ts`; `IdeView.svelte`
in CP2 draws what is there and computes pixel geometry, nothing more. Four invariants hold
after *every* operation, because they all end in the same `finalize` step: empty groups
disappear, a split left with one child collapses into it, a split nested in a split of the
same direction is flattened into its parent, and no child falls under
`MIN_PANE_FRACTION` — without the flattening the stored tree grows deeper with every dock
while drawing identically, and without the floor a pane can be dragged to a sliver with no
reachable splitter to drag it back. Sizes are fractions of their split, never pixels, so a
layout saved on a 21:9 monitor still opens on a 16:9 one. `parseLayout` is tolerant the way
`core/persist.ts` is — unknown node types, tabs without an id or kind and repeated ids are
dropped or renamed rather than trusted — and returns `null` when nothing usable survives, so
a truncated `layout_json` opens the starting layout instead of an empty IDE that looks like
data loss.

**CP2 is the view.** `apps/axiomata/src/ide/` now holds `IdeView.svelte` (full screen, owns
the layout and every drag), `DockNode.svelte` (the tree, drawn recursively; a child's
`flex-grow` *is* its stored fraction, so there is no second place a size lives),
`PaneGroup.svelte` (tab bar, stacked panes, drop highlight) and `panes/PaneHost.svelte`. The
entry point is the IDE icon in the icon bar. Two files keep logic out of the components:
`dock.ts` turns a pointer position into a dock target (edge zone a quarter of a pane, the
whole layout's outer strip 3 %, a tab bar winning over both) from geometry measured **once**
per drag, the snapshot approach `core/kanban.ts` already uses for cards; `moduleAdapter.ts`
hands a pane the same `ModuleContext` a tile gets from `core/registry.ts`, so the Terminal in
a pane is the very same module the canvas mounts — with its config living in the layout tree
rather than `dashboard.json`, and `requestResize` deliberately a no-op because in a dock the
tree owns the sizes.

Two rules that are not obvious from the code and will bite whoever ignores them: **a pane is
never unmounted, only hidden** — not on tab switch, not on leaving the view (`App.svelte`
keeps `IdeView` mounted from the first open), and not when the layout is rearranged — because
the terminal closes its PTY session in `onDestroy`, and it is hidden with `visibility`, never
`display: none`, so it keeps the size its `ResizeObserver` reports to that PTY. And **Escape
does not close the IDE**: it belongs to whatever runs in the pane.

That first rule needed `ide/paneStore.ts` to actually hold, which arrived after M7.2 CP4 and
an owner report of three agents restarting mid-drag. Svelte cannot move a component between
two `{#each}` blocks, and the dock tree is rendered recursively — so every structural change
(docking to an edge, dragging a tab into another group) changed which block a pane belonged
to, and Svelte honoured that the only way it can: by destroying the pane and building a new
one. For a terminal that means a closed PTY and a restarted agent, and it happened to *every*
pane on screen, because inserting a split moves its neighbours a level deeper too. So panes
are not rendered in the tree at all: they live in one flat store that is never reordered, the
tree holds empty slots, and each pane is *moved* into its slot with `appendChild`, which
relocates a DOM node instead of recreating it. Two steps, because a slot being destroyed
would take the pane inside it along: park every pane back in the store in `$effect.pre`
(before Svelte touches the DOM), place them into the new slots in `$effect` (after). The
store fills the dock area and is hidden with `visibility`, so a pane waiting there still
measures its real size. `data-ide-mount` on a pane host is what makes "did that drag restart
it?" answerable from the DOM. The same rule holds *inside* an agent pane: its
side tabs (Plan, Diffs, Inbox) hide the terminal, never unmount it, because
glancing at a plan must not restart the agent.

**CP3 closes M7.1**: the view works in a *project*. Seven thin passthroughs in
`src-tauri/src/commands.rs` expose the CP0 store. Above them sit two modules rather than a
fatter component: `ide/projectSession.ts` holds which project is open and everything that
changes that (a `core/boardStore.ts`-shaped store, so the rules are unit-tested rather than
trapped in a `.svelte` file), while `IdeView.svelte` keeps the dock tree and the two drag
engines and nothing else — the seam was cut here deliberately, at the end of M7.1, because
M7.2's agent panes need to hook into project switching too. Two rules in that module are
ones the checkpoint's reviews put there: an `openProject` answer that a newer switch has
overtaken is discarded (the same sequence guard `terminal.svelte` uses for scrollback), and
a pane's `cwd` is recomputed from the project's *current* folder on every open
(`ide/paneCwd.ts`) instead of trusting the value frozen into the stored layout — "Change
path" would otherwise leave terminals starting in the folder the project just left.
`ide/projects.ts` wraps the commands and owns the one rule about timing — a layout is
written debounced (400 ms, the rhythm `core/persist.ts`
already uses), but a pending write for a different project is flushed before another is
queued, because switching projects while one waits is exactly how an arrangement gets lost.
Opening a project loads its `layout_json`, or builds a starting layout when it has none; the
old project's panes are unmounted, which is the one place in this view where destroying a
pane is right, since those terminals were running in a different folder.

Two consequences worth knowing. **Without a project there are no panes** — the IDE asks for
one rather than opening a terminal with nowhere to be. And **a terminal pane starts in the
project's folder**: `modules/terminal.svelte` now prefers a host-supplied `cwd` from
`ctx.config` over the global `terminalSettings.cwd`. That is not a reversal of Checkpoint 5d
(which moved every *setting* out of `ctx.config`): `cwd` here is not a setting but the fact of
where the pane lives, nothing sets it on the canvas, and M7.2 puts the agent's worktree in
the same place. Full plan in [`plans/agentic-ide.md`](plans/agentic-ide.md) §5.

**M7.2 has begun.** CP4 adds *agent profiles*: `ide_agents` (migration 10, the crate's frozen
`SCHEMA_SQL_V2`) holds a name, a harness (`claude_code` | `opencode` | `mini`), a command
line, a model and an env block, with `axiomata-cli ide agents …` and a menu in the IDE header
to manage them. `panes/AgentPane.svelte` runs one: the harness in a terminal, a status line,
a Restart button that remounts the terminal, and the side tab bar the milestone exists to
build — Terminal today, Plan/Diffs/Inbox showing what they wait for.

Four decisions worth knowing. The harness is **typed into a shell** rather than spawned
directly, so a profile may hold a real command line and the output survives the agent
exiting. The opening command travels as a **prop, never through `ctx.config`** — a pane's
config comes back out of a stored `layout_json`, and a command there would mean opening a
project runs whatever the layout says (the security audit for this checkpoint named the
shape; `ide/paneCwd.ts` applies the same rule to `cwd`). An agent name is unique per project
**ignoring case**, because CP5 turns it into a directory and macOS does not distinguish.
And `effective_command` is computed in Rust and sent along, like a project's `root_exists`,
so no frontend keeps a second copy of the harness-to-command table.

**CP5 gives each agent its own git worktree** — and with it, the repository's first git
integration at all. `crates/axiomata-ide/src/worktree.rs` drives the `git` command line
(question F3, answered here rather than in M7.3 because worktrees came first): `git worktree`
is the reference implementation of something libgit2 only partly models, the user's own
configuration — credential helpers, `includeIf`, hooks — applies for free the moment an agent
commits, and there is no C dependency in a crate meant to stay extractable. The price is that
`git` must be installed, which `ensure_available` reports as a sentence.

`provision.rs` is the one place that knows a worktree and a port belong together. It is
idempotent and runs on **every** start, not only at creation, so an agent that predates CP5
or whose directory somebody deleted repairs itself by being started. Worktrees live at
`~/.axiomata/worktrees/<project>/<agent>-<id>` — app-owned runtime data, deliberately outside
the user's repository so an agent's checkout never shows up in their own `git status`. The id
is in the path because a slug is not unique: "A B" and "A-B" would otherwise be one directory
and two agents overwriting each other.

Three rules worth knowing. A project that is **not** a repository is an ordinary case, not a
failure: its agents share the project folder as they did before, still get a port, and the
status line says "shared folder". A port is reserved in the database (a partial UNIQUE index,
since most agents have none) rather than by holding it open — the agent's own process binds
it, and `AXIOMATA_PORT` is how it learns the number, which is what stops two agents' dev
servers fighting over 1420. And the **identity env is appended last** — `AXIOMATA_AGENT_ID`,
`_NAME`, `_WORKTREE`, `_BRANCH`, `_PORT` after the profile's own lines — so a profile cannot
declare itself to be a different agent: later wins, both in `PtySession::spawn` and in the
frontend's `mergeEnv`.

**CP6/CP6b give each agent a status and a plan** — and, deliberately, still no schema change
(`docs/plans/agent-lifecycle.md`, E9–E20). The status is runtime state that dies with the
PTY, so a database row saying "working" would be a lie after the next restart. Instead
`crates/axiomata-ide/src/lifecycle.rs` owns a **file channel** per agent at
`~/.axiomata/agent-events/<id>/`: `state` (one word — `idle`/`working`/`waiting`/`ended` — and
a Unix timestamp) and `started`. For Claude Code the harness writes, Rust reads:

- **Claude Code** is started with `--settings <channel>/claude-settings.json`, a file of ours
  whose hooks (`SessionStart`, `UserPromptSubmit`, `PostToolUse`, `Notification`, `Stop`,
  `SessionEnd`) are plain `sh` lines writing the state word — no Axiomata binary is needed on
  the agent's `PATH`, which matters because the bundled app does not ship the CLI. The
  `Notification` hook carries no matcher: it `grep`s its own JSON for `notification_type`
  `permission_prompt`/`elicitation_dialog` before reporting `waiting`, because Claude Code
  also notifies on a 60-second idle prompt and a login, neither of which is really waiting for
  an answer (a live test caught an idle agent blinking `waiting` before this). Its **plan** is
  the one exception to "the harness writes our format":
  Claude Code's task tools (`TaskCreate`/`TaskUpdate` — `TodoWrite` no longer exists) only
  report single changes to a hook, so we pin the list with `CLAUDE_CODE_TASK_LIST_ID` and read
  Claude Code's own `~/.claude/tasks/<list>/*.json`, tolerantly.
- **Opencode** reports through its own service since OC3 (`docs/plans/opencode2.md`): the v1
  plugin under `OPENCODE_CONFIG_DIR` died with Opencode 2 (plugin format, config key and event
  names all changed, and the service ignores that variable) and is gone; `reset` removes what
  it left in a channel. `axiomata_core::ide_status` runs one watcher per process — started by
  the first `ide_agent_states` call, never starting the Opencode service itself — that opens
  `/api/event`, then seeds an `axiomata_opencode::Tracker` for every agent session from the
  service (`/api/session/active`, pending permissions and forms, the newest finished
  plan-agent answer), and applies each event: `session.execution.started` / `permission.replied`
  / a closed `form.*` → `working`, `permission.asked` / `form.created` → `waiting`,
  `session.execution.succeeded|failed|interrupted` → `idle`. The plan agent's answer (its
  newest step's `session.text.ended` text when the turn ends) is the agent's plan document.
  `overlay` lays that over the channel's status for Opencode agents on the generated command;
  a session nothing happened in since the watcher connected reads `idle`, and without a
  connection the channel's `starting` stands. On losing the stream it looks again after 2 s,
  doubling to 30 s. The CLI's `ide agents status` takes one fresh look (`overlay_once`). An
  Opencode agent never reports `ended` — the service does not know when a terminal UI exits.

**Opencode agents run in a session the IDE keeps for them** (`docs/plans/opencode2.md`, OC2,
migration 13 `ide_agents.opencode_session`). Opencode 2's terminal UI takes no `--model`, so
`Agent::resolve_command` never appends one for Opencode; instead
`axiomata_core::ide_start::start_agent` — the one start path of the dashboard
(`prepare_ide_agent`) and the CLI (`ide agents prepare`) — runs `provision::prepare`, then
`agents::opencode::ide_session`: the stored session is continued when the service still has it
in the agent's directory (its model switched when the profile's changed), otherwise a new one
is created there with the profile's model, the agent's name as title and the rules
`shell: git push` / `git push *` → `deny` (Q7, "never a push"). The launch command becomes
`opencode --session <id>`; an agent with its own command line gets no session (E13). "New
session" (`ide_agent_new_session`) forgets the id, so the next start creates a fresh one; the
old stays in Opencode's list. No planning instruction for Opencode yet: the text asks for a
todo tool v2 no longer has, and the session-instructions endpoint is experimental — the step
list comes with Axiomata's own MCP server (Q3).

Nothing is ever written into a worktree, so an agent cannot commit its own hookup, and a
shared (non-git) folder gets a status too. `prepare` resets the channel on every start and
returns `launch_command`/`launch_env` — the command gains `--settings` only when it is the
generated one; an own command still gets the env and can attach itself via
`$AXIOMATA_CLAUDE_SETTINGS`. The plan survives an agent restart and is flagged
`from_earlier_session`; deleting an agent removes its channel and exactly its own task list.
Two additions from the live test. Claude Code gets a short **planning instruction**
(`<channel>/planning.md`, via `--append-system-prompt-file`) so
the Plan tab has something to show without being asked, and a **plan-mode plan** is shown as
its own document below the tasks: Claude Code writes it to `<claude-home>/plans/<name>.md`
(a `Write|Edit` hook records the path; Rust only reads it if the canonical path is inside
that folder), Opencode's plan agent answers in chat (the watcher keeps that answer, see
above). And the app strips **inherited Claude Code session markers**
(`CLAUDECODE`, `CLAUDE_CODE_CHILD_SESSION`, … — `forget_inherited_claude_session` in
`src-tauri/src/lib.rs`) at startup: launched from inside a Claude Code shell, every agent
pane otherwise believed it was that session's child and saved no transcript.
The frontend polls `ide_agent_states(project)` once a second while the IDE is on screen
(`ide/agentStatus.ts`) and shows the result as one `StatusDot` in three places: the pane's
status line, its dock tab, and the agent picker. The status is also what M7.5 CP13 will use
as its delivery condition (a message reaches a TUI agent only when it is `idle`).

**M7.3 CP7 adds the git engine** (`crates/axiomata-ide/src/git.rs`, decisions G1–G13 in
`docs/plans/git-layer.md`) — still `git` as a subprocess. It answers "what has this agent
changed?" against the branch its worktree was cut from, now recorded as `base_branch`
(migration 12) when the branch is born; older agents fall back to the project folder's
current branch. The diff starts at the **merge base**, not the base's tip, so work that landed
on `main` meanwhile does not read as the agent reverting it, and it covers committed and
uncommitted work together, each file marked if part of it is still uncommitted. Diffs are
parsed in Rust into hunks and numbered lines, capped at 2 MiB / 10 000 lines.

The handgrips: **discard** puts a file back to the base (committed changes included; files
the agent added are deleted, only inside the worktree), **commit_all** commits what the agent
left lying around, and **take_over** is the only function that touches the user's own working
copy — squash by default, `--no-ff` on request, never a push. It refuses rather than guesses
(another branch checked out in the project folder, anything staged there, uncommitted agent
work, an agent that is `working`/`waiting`), undoes a conflict before returning it as an
ordinary outcome, and afterwards moves the agent's branch to the new base so its diff is
empty. `provision::agent_repo` reads where an agent works under a brief DB lock and hands back
an `AgentRepo`, so no connection is held while git runs; the Tauri commands (`ide_agent_changes`,
`_file_diff`, `_discard`, `_commit`, `_take_over`) are async and run git on a blocking thread.
Every git call runs with `GIT_OPTIONAL_LOCKS=0` (the IDE reads a worktree while the agent in it
runs git itself) and `GIT_LITERAL_PATHSPECS=1` (a file the agent names `:(glob)*` is that file,
not a pattern — the security review showed `--` does not prevent pathspec magic). The busy
check has one owner, `provision::TakeOverTarget::run`, and `AgentRepo::take_over` is
crate-private so no caller can skip it; `agent_repo` answers `Ready | SharedFolder |
NotStarted` so the UI can say why an agent has no diff. A hook-rejected `--no-ff` merge is
rolled back via `MERGE_HEAD`, special files (FIFOs) are never opened, and one refresh costs two
git calls (`status`, one `diff --raw --numstat`).

**M7.3 CP8 puts a Diffs tab on every agent, drawn on the editor** (decisions H1–H16 in
`docs/plans/git-layer.md`). A diff view is a read-only editor with markings (editor plan D15):
the engine's `editor/diff/` turns git's hunks plus both whole sides into rows — unchanged
stretches folded, unfoldable 20 lines at a time or entirely, changed words marked inside
paired lines — and `view.ts` turns the rows into ordinary pane documents with
`LineDecoration`s (row colour, gutter label, word marks, a label with buttons on an empty
row). `EditorSurface` draws decorations generically, so the later Git-Gutter uses the same
hook. **The hunks decide what changed** (git's own, so the view agrees with `+n −m` and a hunk
shown is exactly the hunk that will be discarded); the whole texts only fill the gaps and
feed one tree-sitter highlighter **per side**, because a parser reading removed and added
lines mixed together colours both wrongly. The base side comes from the new
`ide_agent_base_file` (`ls-tree` for the entry, `cat-file` by object id — never
`<commit>:<path>`, whose path part git interprets), the worktree side through the guarded
`file_read` on `worktree:<id>`; pictures show before/after. `fileapp/DiffPanes.svelte` draws
one surface (unified) or two scrolled in step (split, blank rows opposite a missing line) and
is shared with the file app's "Compare", which now diffs the disk against the buffer with an
engine-side Myers (`editor/diff/myers.ts`). One echo guard (`fileapp/scrollLink.ts`) serves
every pair of synced views. `ide/DiffView.svelte` reloads by G4 (`ide/diffRefresh.ts`: coming
into view, the agent stopping, every 5 s while visible and working) and keeps the open file's
folds when its hunks did not change.

**M7.3 CP9 lets you act from the diff.** Two new dock panes (H14, `ide/paneKinds.ts`): `file`
(`{root, rel, line, jump}` — the editor on any root, opened from a diff at the clicked line on
`worktree:<id>`, with a hint while the agent works, G6) and `agent-diff` (`{agentId}`, at most
one per agent). `IdeDock.open` places a new pane beside the one it came from, gathers files in
the group that already holds one, and brings an open one forward instead of doubling it. The
editing itself is `fileapp/FileEditor.svelte`, extracted from the file app so the full-screen
view and the pane edit the same way. Panes are moved in the DOM on every layout change
(`ide/paneStore.ts`), which resets scroll positions silently; elements marked
`data-keep-scroll` get theirs back. The handgrips live in `ide/diffSession.svelte.ts`
(`AgentDiffSession`, a runes class with an injectable backend, tested without a DOM): discard
a file or one hunk (`git.rs` `discard_hunk` rebuilds that hunk as a patch — C-quoted names —
and reverse-applies it through stdin, refusing if the file's diff no longer has that hunk,
H13), commit (G3), take over (G7–G12, blocked in the UI with the reason while the agent works
or has uncommitted work, and refused again in Rust). Messages come prefilled: the plan-mode
heading, else the agent's last commit subject (H10). Discarding several files costs one
`ls-tree`, one `restore` and one `rm --cached`.

### `axiomata-macos`

The integration boundary for macOS-specific features beyond what MCP servers already cover
(§5's connector modules reach Apple Mail/Calendar/Reminders that way). So far only the
pasteboard (`clipboard`, §3), for the editor's Vi registers.

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
- **M4 — Always-on behaviour: dropped** (owner, 2026-09-20). Cancelled outright rather than
  deferred; see §6 for the reasoning.
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
  replacing the raw cron text field; every dashboard tool module made a canvas singleton;
  the Calendar tile's mini-month + selected-day-plus-7 agenda + optional digital/analog
  clock (`docs/plans/calendar-polish.md`); the Terminal module (`axiomata-terminal`, §3) — a
  self-built PTY/VT100 emulator rather than an embedded existing terminal or `xterm.js`,
  deliberately so the ANSI-interpretation and screen-model parts stay a hands-on learning
  project rather than hidden inside a library. Checkpoints 0–4 (PTY spawn, end-to-end IPC
  wiring, ANSI/VT100 + Canvas-2D rendering, scrollback/alternate-screen/selection/paste),
  Checkpoint 5 (font/shell settings), and Checkpoint 5b (scrollback size, start directory,
  extra env vars, cursor style, colour themes, bold-is-bright, visual bell, custom font,
  background opacity) are done. Checkpoint 5d — bug fixes from the first real owner
  live-test, plus moving Terminal settings out of per-instance `dashboard.json` config into
  their own global `terminal-settings.json` (§4 above) so closing a tile no longer discards
  its settings — and Checkpoint 5e — a second live-test round's fixes: an explicit
  `clearRect` against canvas ghosting, a startup size driven off a fixed 120×60-character
  target at the currently configured font (a new optional `ModuleDefinition.computeDefaultSize`
  hook, `core/types.ts`, preferred over the static `defaultSize` by `core/lifecycle.ts`'s
  `createInstance` when a module declares one — so far only Terminal does), and autofocus on
  a freshly spawned terminal — are both done; full detail and status: `docs/plans/terminal.md`.
- **Stufe 2 — the lean local agent (CP1–CP4): shipped, then superseded by CP5.** The
  hand-rolled stdio MCP client + `[mcp_servers]` config + `axiomata-cli mcp import|list|tools`
  (CP1), the `AgentBackend::OllamaAgent` bounded tool-call loop over Ollama + MCP (CP2), the
  provider-driven `local_backend:` switch + `prepend_files:` (CP3), `num_turns` on loop
  results, the Settings hint, `axiomata-cli skills reseed [--force]`, and this doc catching up
  (CP4) were built to run connector digests on a local model without the Claude Code CLI. The
  bake-off (`docs/plans/stufe2-cp3-bakeoff.md`) showed the *harness*, not just the model, was
  the problem — **CP5 replaced the whole stack**: Claude Code and `ollama-agent` are gone, and
  every skill runs through the headless Opencode CLI (`AgentBackend::Opencode`, MCP from
  opencode's own config), which serves cloud *and* local models with one mechanism. Full
  original plan: `docs/plans/stufe2-lean-ollama-agent.md`.
- **Editor ED0 — the file service: done** (2026-09-23, `docs/plans/editor.md`). The
  `axiomata-files` crate (§3), root ids and grants, the `file_*` Tauri commands with the
  Rust-driven open dialog and the `files:changed` watcher event, `axiomata-cli files`, and the
  App Ring's "Ansicht öffnen" entry type (`core/apps.ts` `RING_VIEWS`; `view:ide` today,
  `view:editor` with ED1). Next: ED1, the editor core.
- **Editor ED2 — appearance: done** (2026-09-24): tree-sitter highlighting for 15 languages
  with injections, colours in all five themes, cursor glide with trail, current line, indent
  guides, bracket colours, smooth scroll, the Markdown preview with scroll sync. Next per D15:
  M7.3 CP8/CP9 on the editor, then ED3 (Vi).
- **M7.3 CP8 — the Diffs tab: done** (2026-09-24, §3 `axiomata-ide`): the diff on the editor
  (unified and split, folds, word marks, per-side highlighting, pictures), `ide_agent_base_file`,
  the file app's "Compare" as a real diff.
- **M7.3 CP9 — acting from the diff: done** (2026-09-24): the `file` and `agent-diff` dock
  panes, discard per file and per hunk, commit, take over with prefilled messages. M7.3 is
  complete; next per editor plan D15: ED3 (Vi mode).
- **Editor ED3 — Vi mode: done** (2026-09-25, V1–V12 in `docs/plans/editor.md`).
  ED3.1: the Vi machine in `src/editor/vi/` (pure TS, table-tested). ED3.2: wired to every
  editor surface (`fileapp/viKeys`, `viSurface`, `viScroll`, `viShared`), cursor shapes, the
  mode pill, the Mac pasteboard through `axiomata-macos`. ED3.3: search (incsearch, hlsearch,
  `n N * #`) and the ex line (`:w :q :s :set …`), the command line living inside the machine so
  macros and `.` replay it (`vi/cmdline`, `vi/cmdmode`, `vi/search`, `vi/ex`, `vi/substitute`);
  `ViStatusLine.svelte` in the file app's footer and over diffs. ED3.4: tree-sitter text objects
  `if/af ic/ac ia/aa` from one node-type table across the grammars (`editor/syntax/objects.ts`),
  and `editor-vi.json` (`core::editor_vi`, `fileapp/viPersist.ts`). Next: ED4.
- **Editor ED4 — single point of truth: done** (2026-09-26, W1–W17). ED4.1: the file
  panel (`fileapp/FilePanel.svelte`, a panel-only module `file`) replaced the Document tile
  and viewer (`md-file`): `FileEditor` now shows every file kind (rendered Markdown/HTML/SVG
  beside the source, images, a new note filed with `create_note`), panels ask before closing
  over unsaved text (`core/staging.ts` close guards), and the agent opens a file for the owner
  through the shell-level action `openFile` (`core/registry.ts`, instance id `shell`).
  ED4.2: the Second Brain's detail panel shows files through `FilePeek.svelte`. ED4.3: tabs in
  the file app (`fileapp/tabs.ts`, one `FileTab`/`FileEditor` per tab, hidden ones stay mounted;
  a panel hands its open session over, `handoff.ts`), and an own app menu (`src-tauri/src/menu.rs`)
  so ⌘W closes a tab, not the window. ED4.4: the file tree (`FileTree.svelte`, `treeModel.ts`) on
  `axiomata-files::dir` — list, create, rename (no replace), count, delete a tree — with
  `files:renamed`/`files:removed` events every open editor follows. ED4.5: quick open ⌘P
  (`axiomata-files::index` over the `ignore` walker, `fileapp/quickOpen.ts` ranking,
  `QuickOpen.svelte`). ED4.6: the IDE's Files pane (`ide/panes/FilesPane.svelte`, the same tree on
  the project root; new projects start with it on the left), a Terminal/Files `+` menu per tab
  group, and ⌘P over the open project. Next: ED5.
- **Editor ED5 — tools: done** (2026-09-26/27, T1–T19 in `docs/plans/editor.md`). ED5.1: the
  rope (`editor/rope.ts`, an immutable B-tree of line blocks behind `TextStore`, snapshots for the
  coming search worker); files up to 16 MiB editable, over 2 MiB in a light mode without
  tree-sitter; the recovery folder capped at 256 MiB in total (`MAX_TOTAL_BYTES`). ED5.2: the
  search guard (`editor/search/`): a worker runs every pattern first on a fixed rope with a 1 s
  limit, the main thread only after an ok verdict (or answers from its offsets); Vi pauses its key
  queue for a verdict; loops (`:g`, `3@:`) are vouched for once up front. Vi gained `:g`/`:v`,
  `:d`, `:normal`, `:s///c` (`vi/confirm.ts`), line-spanning `:s`, `gn`/`cgn`. ED5.3: multiple
  cursors (`editor/multicursor.ts`): `EditorDocument.extra` beside the main `selection`, undo steps
  keep all of them; a command runs at each cursor last-to-first as one step (⌥↑/↓ per block of
  touching lines); ⌥-click/-drag, ⌥⌘↑/↓, ⌘D/⌘U, ⇧⌘L, Esc; multi-aware copy/cut/paste.
  ED5.4: the find bar (`fileapp/FindBar.svelte` over every surface; logic in
  `editor/search/findModel.ts`, language in `editor/search/find.ts` — Vi's pattern language):
  ⌘F/⌥⌘F/⌘G/⌘E (also in Vi), regex/case/whole word, in selection, `$1`/`$&`, preserve case,
  replace all as one step, ⌥⏎ matches → cursors; replace only where the surface is editable and
  not a diff; the last search is shared between bars and Vi (`fileapp/findShared.ts`).
  ED5.5: folding (`editor/fold/`): `ranges.ts` works out where the text folds (indentation always,
  the tree — the text-object node table plus bracketed nodes and multi-line comments — or Markdown
  headings), `state.ts` (`FoldState`) holds the closed folds as line ranges that follow every edit;
  `VisualLayout` gives folded lines no rows, ↑/↓/←/→ and Vi's `j`/`k` step over a fold, linewise Vi
  operators take it whole. The surface draws a gutter chevron and "⋯ N lines", takes ⌥⌘[ ⌥⌘] ⌥⌘0
  ⌥⌘J and Vi's `zc zo za zM zR`, and opens whatever hides a cursor after every command (search,
  undo, jumps). Each `FileSession` owns its folds; `fileapp/foldMemory.ts` keeps them per file in
  `settings.editor.folds` of `dashboard.json`, forgotten when the tab or IDE pane closes. A split
  diff's panes share one `FoldState`.
  ED5.6: sticky scroll (`editor/sticky.ts`: up to five headers of the fold ranges around the top line,
  pinned over the text; the cursor, `zt` and `H` stay below them) and the minimap (`editor/minimap.ts`
  geometry, `fileapp/Minimap.svelte` canvas right of the scroller: token-coloured ink, slider, search and
  diff marks). Both are settings (on), off in the floating panel, `FilePeek` and the light mode.
  ED5.7: the project search: `axiomata-files::search` (the quick-open walk plus include/exclude globs,
  every file read through the guarded `read_text`, `regex` crate, ≤ 10 000 matches), Tauri
  `file_search` streaming batches over an `ipc::Channel` (a newer query per `owner` stops the older),
  CLI `files search`; `fileapp/ProjectSearch.svelte` in the file app's Files | Search column (⇧⌘F)
  and as the IDE's `search` pane; `fileapp/dirtyFiles.ts` marks files with unsaved changes.
  ED5.8: installed Mac fonts: `axiomata-macos::fonts` (CoreText via raw FFI, no crate), Tauri
  `installed_fonts` (cached per run), `core/installedFonts.ts`; the editor's and the terminal's font
  pickers offer them (the terminal monospaced only), a missing chosen font draws the default.
  ED5.9: moving in the tree: drag a file or folder onto a folder of the same root (pointer events,
  `treeModel.moveTarget`, the existing `file_rename`); a closed folder opens after half a second.
- **Editor ED6 — language servers: under way** (2026-09-28, `docs/plans/editor.md` "ED6 im
  Detail", L0–L11). L0: `$HOME`, `/Users` and `/` are refused as IDE project roots
  (`store::too_wide`; with no resolvable `$HOME` no root is accepted at all). ED6.1 (server host + protocol base + diagnostics): Rust side above
  (`axiomata-files::lsp`); the protocol is the engine's, `src/editor/lsp/` (no app imports):
  `rpc.ts` (JSON-RPC, answers the server's own requests with `null`/per-item `null` for
  `workspace/configuration`), `client.ts` (`initialize` with UTF-16 positions; a document is
  opened with its text as on disk and then follows `EditorDocument.onTextChange` —
  incremental ranges when the server takes them, the whole text otherwise — sent after a
  150 ms pause and before every request), `diagnostics.ts` (`DiagnosticSet`: stretches per
  line, worst severity per line, what covers a position, next/previous). `fileapp/lsp.ts`
  keeps one client per (root, server) for the page and shows the install hint once per
  server; `FileEditor` opens the file on its server in the full-screen app and the IDE only
  (not the panel, not the light mode, not a new note), and `EditorSurface` draws wavy
  underlines (`--ax-diag-*`), a gutter dot, the messages under a resting mouse and — with the
  setting "Problem message at line end" (off) — the worst message after the line. F8/⇧F8
  and Vi's `]d`/`[d` (a `problem` effect) step through; the status line counts them. ED6.2:
  hover and definition — `LspClient.hover` (`hoverMarkdown` normalises every contents shape;
  the client asks for Markdown) and `.definition` (`firstLocation` of a `Location`, a list, or
  `LocationLink`s). The surface shows the server's note (through `renderMarkdown`/DOMPurify)
  under the problems when the mouse rests, and on Vi's `K` below the cursor
  (`showHoverAtCursor`); F12, ⌘-click and Vi's `gd` go to the definition: in the same file
  the cursor moves, in another file of the root the host opens a tab (`onOpenFile` —
  `FileAppView`'s tabs, the IDE's dock), outside every root a read-only tab on
  `lsp:<handle>` (`definitionFile`; `FileSession.readOnly` never saves or keeps text aside,
  and such files stay out of "recently opened"). ED6.3 (L12): implementation (⌘F12, Vi `gri`),
  type definition (`grt`) and uses (⇧F12, `grr`) through `LspClient.locations`; one place is gone to like
  a definition, several become a `LocationList` (`fileapp/locationList.ts`) that the project search's result
  list shows (`ProjectSearch.showLocations`) — in the file app's Search column, or in the IDE's Search pane,
  reached through `IdeDock.showLocations` and the `pendingLocations` store. ED6.4: completion — the
  engine's `editor/lsp/completion.ts` parses answers, filters fuzzily, expands snippets (L9) and turns a
  taken item into the `complete` command (the word replaced plus extra edits such as an auto-import, one
  undo step; in Vi it runs inside the Insert session, and `.` repeats it without the import);
  `fileapp/completionMenu.ts` is the menu's behaviour (opens while typing a word or after a trigger
  character, follows the word, drops late answers, resolves an item before taking it) and
  `CompletionPopup.svelte` draws it; `EditorSurface` owns the keys (⌃Space, ↓/↑, ⌃N/⌃P, ⌃Y, ⌃E, Esc; ⏎/⇥
  only without Vi). ED6.5: formatting — `axiomata-files::format` runs the formatter table (after the owner's
  conform.nvim: ruff, rustfmt with the crate's edition, prettier, stylua, shfmt; stdin/stdout, 10 s limit;
  overridable only in `lsp.json` `"formatters"`; Prettier's configuration is looked up by Rust between the
  file and its root only — `--config`/`--no-config` — so no `prettier.config.js` above the project runs; at
  most 4 formatters at once) behind Tauri `file_format`, the language server's
  `textDocument/formatting` as the fallback; the result goes in as the lines that differ (`editor/textEdits.ts`,
  the `replaceText` command), and ⌘S/`:w` format first unless off (`formatOnSave`, `formatOnSaveExcept`). Rename
  (F2, `grn`, `:rename`): `prepareRename` + `rename`, applied by `fileapp/workspaceEdit.ts` — open documents as one
  step each (another editor's surface redraws on `docTouched`), closed files through the file service with a
  version check, nothing outside the roots; the client declares no resource operations, so a rename that would
  move files is refused by the server. The search path ends in `~/.local/share/nvim/mason/bin` (L15).
  Every tool the editor starts — language servers, formatters, `rustc --print sysroot` — runs in an
  allow-listed environment (`axiomata-files::toolenv`: home, user, locale, temp folder, the toolchains' own
  variables; `PATH` = the editor's search path plus the system folders), never the app's whole one: no API
  keys, no `NODE_OPTIONS`. ED6.6: signature help — `editor/lsp/signature.ts` parses it,
  `fileapp/signatureHint.ts` opens it on the server's trigger characters (and ⇧⌘Space) and follows the
  typing until the server answers with nothing; `EditorSurface` draws it above the line. ED6.7: code actions
  (⌘., ⌘L where a server offers them — else ⌘L still selects the line —, Vi `gra` incl. Visual range,
  `:action`, a "Fix…" button in a problem's hover): `editor/lsp/codeActions.ts` parses and orders them,
  `fileapp/codeActionMenu.ts` + `CodeActionPopup.svelte` are the menu (numbers 1–9, unscharf filter),
  `FileEditor.takeCodeAction` resolves, applies the edit and runs the command. **The one server command the
  page may send is an echo** (L18): Rust reads every `textDocument/codeAction` answer and lets
  `workspace/executeCommand` through only for a command (name + arguments, numbers as doubles) the server
  offered there, once each — resolve answers offer nothing, since the page chooses what is resolved. The
  server's `workspace/applyEdit` is honoured only while a command of ours runs, plus 1 s (L23).
  `workspaceEdit.ts` reads every closed file first and changes nothing when one is missing (TypeScript's
  "Move to a new file" writes into a file that does not exist yet).
- **Editor ED1 — the editor core: done** (2026-09-24, §3 "The editor"). Model, surface with
  soft wrap and IME input, the full-screen view with save/external-change/recovery flows,
  settings with every real font weight, autosave. Next: ED2 (tree-sitter, themes, the
  eye-candy of D8, Markdown preview).

Each milestone from M1 onward was broken down into a detailed, step-by-step implementation
plan shortly before it was actually started, rather than all at once up front — those plans
live outside this repository, in the owner's local Claude Code planning notes, since their
value is in guiding the work in progress rather than as a permanent record once it lands.

A related owner decision (2026-09-08) governs the *cadence* of the mandatory automatic
sub-agent checks (rust-test-engineer, rust-dependency-auditor, rust-performance-analyzer) and
of Claude's own `cargo build`/`clippy`/`fmt`/`test` verification loop: during an iterative
multi-phase implementation (e.g. working through a `docs/plans/*.md` checkpoint list) they
fire **once per plan checkpoint and always before a commit**, not after every individual edit
— running the full ladder mid-task burns session-usage budget for little marginal benefit once
tests are already being written inline as each function lands. The trigger stays mandatory;
only its timing is batched. This is a project-local override, not an edit to the global agent
definitions: a fresh Rust project without it keeps the tighter per-edit cadence.

### Debug (#51), 2026-10-02

Crate `axiomata-dap` (DAP client, `debug.json`, Python under debugpy — tested against a real debugpy with
`AXIOMATA_TEST_DEBUGPY_PATH=<dir with debugpy> cargo test -p axiomata-dap`), Tauri glue `src-tauri/src/debug.rs`
(one session at a time), frontend `ide/debug*.ts`, `ide/breakpoints.ts`, `ide/DebugPanel.svelte`, gutter
breakpoints in `fileapp/EditorSurface.svelte`. Details: `docs/plans/editor-projekt-werkzeuge.md` (#51).

### Status log (moved out of `AGENTS.md`, 2026-10-02)

`AGENTS.md` is loaded into every agent turn and had grown past 30 KB, most of it this running status
paragraph. It is kept here verbatim as the history it is; `AGENTS.md` now carries a short current-state
summary and points here and to the plans.

Axiomata-OS is an early-stage Rust + Tauri desktop app: a personal "Agentic OS" command
centre / second brain, built around the **ARMS framework** (Applications, Routines, Memory,
Skills — see `ARMS-Agentic-OS-Guide.pdf`, though the actual design has since diverged from it
in several places; `docs/architecture.md` §1 explains how).

Milestones **M0–M3, M5, M6** are complete (scaffold, skills runner, memory router, routines
scheduler w/ full CRUD, the Svelte module-canvas dashboard, the particle-graph Second Brain),
plus ongoing post-M6 feature work (ToDo, Calendar/Reminders/Mail connector modules, themes,
the `srcdoc` HTML viewer, the Terminal module). **M4 (always-on/background scheduling) was
dropped outright** (owner, 2026-09-20 — the app being open, or tiles refreshing on open, is
enough); do not plan around it. Full detail: `docs/architecture.md` §5 (what exists) and §7
(milestone history). Detailed step-by-step milestone plans live outside this repo, in the
owner's local Claude Code planning notes — read them before starting new M0–M6-scale work if
available.

**M7 — the agentic IDE is under way** (`docs/plans/agentic-ide.md`): M7.1 is complete (the
`axiomata-ide` crate with projects, the dock-layout model, the full-screen IDE view, and
per-project layouts), and M7.2 is under way: agent profiles (CP4), a git worktree plus reserved port per agent
(CP5, the repo's first git integration — `git` as a subprocess, not `git2`), and a live status plus Plan tab
per agent (CP6/CP6b, `docs/plans/agent-lifecycle.md` — a file channel under `~/.axiomata/agent-events/`, no DB
column; nothing is ever written into a worktree). M7.2 is done; **M7.3 (git layer, `docs/plans/git-layer.md`)** is
under way: CP7's git engine (diff against the recorded base branch, discard, commit, take-over into the project
folder — squash by default, never a push), CP8's Diffs tab (drawn on the editor, H1–H16) and CP9 (file and
agent-diff dock panes, discard per file/hunk, commit, take-over dialogs) are built — M7.3 is complete. The plan below
describes the whole chain: an own full-screen IDE
view with a dock/split/tab layout, foreign agent harnesses (Claude Code, Opencode) hosted as
PTY tiles, A2A over an own MCP server rather than screen-scraping, one git worktree per
agent, a "Plan" tab per agent showing what it is working on, and an own mini-harness —
designed from the start to be extractable into a standalone app the way `axiomata-terminal`
is. Seven milestones (M7.0–M7.6), and **M7.0 is a standalone Kanban module that deliberately
ships before the IDE** — it is useful on its own and is the data layer the agents' task
board later sits on. The eight load-bearing decisions are settled in §3 of that plan.
**Under way:** the file app / own AAA editor (`docs/plans/editor.md`, decisions D1–D19 —
TS engine + `axiomata-files` crate, tree-sitter WASM, Vi mode, LSP, one App-Ring icon each for Editor and
IDE); it slots in before M7.3 CP8 (ED0–ED2 first, CP8's diff view is built on it). **ED0 (file service,
E1–E12) and ED1 (editor core, F1–F13) are done**: `axiomata-files`, root ids + dialog grants, `file_*`
commands, the watcher, the ring's "Ansicht öffnen" entries; the TS engine in `src/editor/` and the file
app in `src/fileapp/` (full-screen view, recovery, settings); **ED2** (tree-sitter highlighting, themes,
effects, Markdown preview) is done too. Per D15, M7.3 CP8/CP9 on the editor are done, and so is **ED3 (Vi, V1–V12)**: the machine in
`src/editor/vi/` (the `:` line lives inside it — `vi/cmdmode.ts`), wired to every surface, the Mac pasteboard
via `axiomata-macos::clipboard`, tree-sitter text objects (`editor/syntax/objects.ts`), and
`~/.axiomata/editor-vi.json` for registers, file marks and histories. **ED4 (single point of truth, W1–W17)**: ED4.1 replaced the Document tile/viewer (`md-file`) with the panel-only file panel
(`fileapp/FilePanel.svelte` around `FileEditor`, opened via `core/staging.ts` `openFilePanel`), and the agent
opens files for the owner through the shell action `openFile` (`core/registry.ts`, instance id `shell`).
ED4.2–ED4.6 are done too — **ED4 is complete**: the Second Brain shows files through `FilePeek`, tabs in the file
app (`fileapp/tabs.ts`; `src-tauri/src/menu.rs` drops the menu's ⌘W "Close Window"), the file tree on
`axiomata-files::dir` (renames and deletes are broadcast as `files:renamed`/`files:removed`), ⌘P quick open
(`axiomata-files::index`), and the IDE's Files pane (`ide/panes/FilesPane.svelte`, ⌘P over the project). **ED5
(tools, T1–T19)** is under way: ED5.1 put documents on an immutable rope (`editor/rope.ts`; `LineStore` stays as the
tests' reference) and made files up to 16 MiB editable, over 2 MiB in a "light mode" (`FileSession.light`, no
tree-sitter). ED5.2: every search pattern is first run by a worker with a 1 s limit (`editor/search/guard.ts`
`SearchGuard`, a verdict per rope version; Vi's key queue pauses on `SearchPending` like on the clipboard), plus Vi's
`:g`/`:v`/`:d`/`:normal`, `:s///c`, `gn`/`cgn`. ED5.3: multiple cursors in normal mode (`editor/multicursor.ts`:
a command runs at each cursor as one undo step; `EditorDocument.extra` + `extraGoals`). ED5.4: the find bar
(`fileapp/FindBar.svelte`, logic in `editor/search/findModel.ts`; the bar and Vi share the last search via
`fileapp/findShared.ts`). ED5.5: folding (`editor/fold/`: `ranges.ts` where the text folds, `FoldState` what is
folded; folded lines have no rows in `VisualLayout`; kept per file in `settings.editor.folds`,
`fileapp/foldMemory.ts`). ED5.6: sticky scroll (`editor/sticky.ts`) and the minimap (`editor/minimap.ts`,
`fileapp/Minimap.svelte`), off in the floating panel (`FileEditor` `compact`). ED5.7: the project search
(`axiomata-files::search`, Tauri `file_search` over an `ipc::Channel`, `fileapp/ProjectSearch.svelte` in the file
app's Files | Search column and the IDE's `search` pane, ⇧⌘F). ED5.8: installed Mac fonts
(`axiomata-macos::fonts`, CoreText through raw `unsafe extern` FFI; `core/installedFonts.ts`). ED5.9: moving in the
tree by dragging (`FileTree.svelte`, pointer events, `treeModel.moveTarget`) — **ED5 is complete**. **Opencode 2
(`docs/plans/opencode2.md`, OC1–OC4) is done**: Axiomata is a client of Opencode 2's shared background service
(crate `axiomata-opencode`) — skills and chat run as sessions on it, IDE Opencode agents start on a session the IDE
keeps (`opencode --session <id>`), and their status comes from the service's event stream. **ED6 (LSP) is under way**
(`docs/plans/editor.md` "ED6 im Detail", L0–L11): L0 refuses `$HOME` and above as a project root; ED6.1 runs language
servers from Rust (`axiomata-files::lsp` — which program, only from a built-in table or `~/.axiomata/lsp.json`; only
the methods the client speaks pass) with the protocol in `src/editor/lsp/`, and shows diagnostics; ED6.2 adds hover
and go-to-definition (a definition outside every root opens read-only on the root `lsp:<handle>`, readable only for
files the server named); ED6.3 adds implementation, type definition and uses (a list in the project search's
results); ED6.4 completion (a menu while typing, as blink.cmp in the owner's Neovim); ED6.5 formatting (own
formatter table in `axiomata-files::format`, after the owner's conform.nvim) and rename; ED6.6 signature help;
ED6.7 code actions (L18–L24: `workspace/executeCommand` only as an echo of a command the server offered in a
`textDocument/codeAction` answer, checked in Rust; `workspace/applyEdit` only while our command runs) —
**ED6 is complete** — and the owner's live test of ED6 was accepted on 2026-09-29, as were the ED5
live test and the ED2 colour sign-off. Nothing in ED0–ED6 is open.
Next in the editor: **ED7** (extraction into a standalone app, the engine already imports nothing
from the app — D1). Beyond that the editor has two follow-on plans of their own:
[`editor-look.md`](docs/plans/editor-look.md) (LK0–LK5, complete — the modern look becoming the
app's standard) and [`editor-projekt-werkzeuge.md`](docs/plans/editor-projekt-werkzeuge.md)
(project roots in the tree, outline, git panel, run/tasks, debug/DAP — grilled 2026-09-30, Q1–Q27;
**#48 git panel is built (status, stage/unstage incl. hunks, discard, commit, push, branches, `git init`; also as an IDE pane) (new crate `axiomata-git` — the only place that runs `git`, shared with `axiomata-ide`; panel in `fileapp/GitPanel.svelte`, change view `GitDiffView.svelte`, commands `src-tauri/src/git.rs`; Push / Commit & Push publish only the checked-out branch to its upstream or `origin`, never forced — the IDE's agent layer still never pushes). #49 outline is built too (`editor/syntax/outline.ts` from the syntax tree / Markdown headings, `fileapp/OutlinePanel.svelte` under the tree, breadcrumbs in the header). #47 is built as project new/open/close — one `projects` registry for editor and IDE, the folder from the native dialog (`project_open`/`project_new`), the tree shows the open project only; the rest stays parked**). `docs/plans/editor.md` lists both under „Fortschreibungen".
**Editor and IDE are becoming one workbench** (`docs/plans/workbench.md`, owner 2026-10-02: one app, a switch Editor / Agents, two layouts per project, agents keep running when switched away). Steps 1 and 2 are built — `ide/dockDrag.svelte.ts` and one shared `ide/DockNode.svelte` serve both views, and `fileapp/ProjectSidebar.svelte` (project bar, Files | Search | Git, outline) is the left column of both (own prefs per view: `settings.editor.tree` / `settings.ide.tree`; the IDE no longer starts with a Files pane, its header has no project picker). Steps 3 and 4 are built: the IDE view *is* the workbench — the editor's tab keys, unsaved-text question, rename-following, preview tab, ⌘N, file-panel handoff, and the **Editor | Agents switch** (two layouts per project in one `layout_json`, `ide/modes.ts`; the hidden mode's panes stay mounted and running). The workbench is named **Studio** (owner, 2026-10-02): one ring entry (`shell:studio`, type id `view:ide`; the old `view:editor` is migrated on load, `core/ringTypes.ts`) opening in the project's last mode; `shell:ide`/`shell:editor` still open it in the Agents/Editor mode (`ide/modeRequest.ts`). The old `fileapp/FileAppView` is gone. Internal names (`ide/`, `IdeView`, `axiomata-ide`) stay until the extraction. The left edge is an activity rail (`ide/ActivityRail.svelte`: views of the code — Files, Search, Git; views for running — Run, Agents; at the foot the actions Terminal and Open file) choosing what the sidebar column shows (`settings.ide.tree.view`; Agents = `ide/AgentsPanel.svelte`); dock groups have no `+` menu. First Mac test done 2026-10-02 (rail follows from it); the header shows the front file's symbol breadcrumbs. **Run/Tasks (#50) first cut** (`axiomata-tasks` crate: detected tasks + `tasks.json` + hash-confirmed project file; rail *Run* view; task panes are terminals whose command line lives only in memory, `ide/taskRuns.ts`; glue `src-tauri/src/tasks.rs`) — see `docs/plans/editor-projekt-werkzeuge.md`.
Deferred meanwhile, by the same owner decision: the ⌘K spotlight search
(`docs/plans/spotlight-search.md`) and further model-provider work (the current Opencode +
OpenRouter setup is considered good enough).

### Status snapshot (moved out of `AGENTS.md`, 2026-10-04)

`AGENTS.md` had regrown a long status paragraph again; this is that paragraph, verbatim, as of the date above. `AGENTS.md` keeps only the short summary and points here.

Axiomata-OS is an early-stage Rust + Tauri desktop app: a personal "Agentic OS" command
centre / second brain, built around the **ARMS framework** (Applications, Routines, Memory,
Skills — see `ARMS-Agentic-OS-Guide.pdf`, though the design has since diverged from it;
`docs/architecture.md` §1 explains how). The long milestone-by-milestone history lives in
`docs/architecture.md` §5 (what exists), §7 (milestones) and its "Status log"; each feature has a
plan in `docs/plans/`. Read those before substantial new work, and **update them** (not just this
file) when a milestone or major feature lands.

**Done:** M0–M3, M5, M6 (scaffold, skills runner, memory router, routines, the module-canvas
dashboard, the Second Brain graph) and post-M6 work (ToDo, Calendar/Reminders/Mail connectors, themes,
the `srcdoc` HTML viewer, Terminal module). **M4 (always-on scheduling) was dropped** (owner, 2026-09-20) —
do not plan around it. M7.0–M7.3 (Kanban, IDE shell, agent panes with worktrees/status/plan, git layer),
the editor ED0–ED6 (own TS engine in `src/editor/`, `axiomata-files`, tree-sitter, Vi, LSP) and Opencode 2
(`docs/plans/opencode2.md`) are complete.

**The Studio** (`docs/plans/workbench.md`) is the one full-screen workbench that replaced the separate
Editor and IDE views (owner, 2026-10-02): `ide/IdeView.svelte` with an **activity rail**
(`ide/ActivityRail.svelte`: Files, Search, Git | Run, Agents | foot: Terminal, Open file) choosing what the
sidebar column shows (`fileapp/ProjectSidebar.svelte`; `settings.ide.tree.view`; Agents = `ide/AgentsPanel.svelte`,
Run = `ide/TasksPanel.svelte`), a dock of panes (`ide/DockNode.svelte`, `ide/dockDrag.svelte.ts`), and an
**Editor | Agents switch**: two dock layouts per project in one `layout_json` (`ide/modes.ts`); the hidden
mode's panes stay mounted, so agents keep running. One ring entry (`shell:studio`, type id `view:ide`; the old
`view:editor` is migrated on load, `core/ringTypes.ts`); `shell:ide`/`shell:editor` open it in the Agents/Editor
mode (`ide/modeRequest.ts`). Internal names (`ide/`, `IdeView`, `axiomata-ide`) stay until the extraction.
Project tools: git panel (#48, crate `axiomata-git` — the only place that runs `git`; Push/Commit & Push publish
only the checked-out branch, never forced; the agent layer never pushes), outline + breadcrumbs (#49), projects
new/open/close (#47, one `projects` table for everything), **Run/Tasks (#50)** (crate `axiomata-tasks`: detected tasks,
`tasks.json` in the project or `~/.axiomata`, project file runs only after a hash confirmation; task panes are terminals
whose command line lives only in memory, `ide/taskRuns.ts`; glue `src-tauri/src/tasks.rs`).

**Debug (#51)** exists for Python (debugpy), Rust, C/C++ and Swift (`lldb-dap`) — crate `axiomata-dap`, `ide/DebugPanel.svelte`, gutter breakpoints with conditions/hit counts/log points, watch expressions, Rust tests and panic stop; `debug.json` runs only after a hash confirmation, like Run. **Node/TypeScript is parked** (owner, 2026-10-02): js-debug needs TCP + child sessions and cannot be fetched/tested on the dev box (`docs/plans/editor-projekt-werkzeuge.md`).

**Next / open:** the owner's Mac test of the Studio, of Run/Tasks and of Debug (the Tauri glue in `src-tauri` is not
compilable on the Linux dev box); #50 follow-ups (clickable `file:line` errors, problem matchers);
**agent-to-agent communication (M7.5)** — planned and approved 2026-10-03, nothing built yet: `docs/plans/a2a.md` (engine / agent /
session, the Flow mode, MCP transport with the A2A data model, build plan CP-A1…CP-A10; start with CP-A1); ED7 (the editor/Studio as a standalone app); a Mac-only-code split for Linux/Windows. Deferred by
owner decision: ⌘K spotlight search (`docs/plans/spotlight-search.md`) and further model-provider work.

### A2A CP-A2 built (2026-10-04)

The board's agent flow (see the `axiomata-board` section in §3): column roles, plans, card fields, dependencies, history and derived state; migration 15; CLI and Kanban UI. Nothing starts from it yet — that is CP-A6.

### A2A CP-A1 built (2026-10-04)

Engines and roles exist (`docs/plans/a2a.md`, "CP-A1 im Detail"): crate `axiomata-roster`, migration 14, `axiomata-core::roster`,
CLI `ide engines …` / `ide roles …`, the Studio inspector's *Agents* tab (`ide/AgentsSettings.svelte` over
`ide/EnginesSection.svelte` and `ide/RolesSection.svelte`, logic in `core/roster.ts`; opened from the Agents panel's
"Engines & roles…") and the confirmation notice for a project's own roles in the Agents panel
(`ide/ProjectRolesNotice.svelte`). They sit in the Studio's own settings column, **not** in the app's general settings
(owner, 2026-10-04): the Studio is to become a program of its own, and its settings move with it. Nothing starts from an engine or a role yet — that is CP-A6. Also this day: the app folder
`apps/dashboard` became `apps/axiomata` (bundle identifier `com.axiomataos.app`); the dashboard *module canvas* keeps its name.
