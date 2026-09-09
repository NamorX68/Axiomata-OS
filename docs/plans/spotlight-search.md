# Plan: Spotlight search (⌘K)

Status: **draft, not started.** Follows the owner's stepwise workflow — settle the "Open
decisions" first, then confirm each checkpoint before the next.

## Why

The top-bar 🔍 icon used to focus the chat input; it now opens the Second Brain with its
search box focused (quick fix, shipped 2026-09-09). That's the *graph-context* search
("find a note and see how it connects"). The owner also wants a **fast, keyboard-first
finder** — a ⌘K overlay that jumps to anything: a note, a skill, a routine, a module to
place, a shell action — without leaving the current view.

## What exists to build on

- `search_workspace` (Tauri command) — full-text over the tracked `.md`/`.html`/`.txt` files,
  returns `SearchHit[]` (path, line, snippet, match count). Used today by the SB search.
- `list_skills` / `list_routines` — already commands.
- The module registry (`core/registry.ts`, `listModules()`) — client-side.
- `core/commands.ts` — the `/command` table (route/runCommand), client-side.
- `core/staging.ts` `openStaged("md-file", …, "right")` — opens the (now centred) file viewer.
- `open-second-brain` bus event — reveal a node / run a query in the SB.
- The shell bus (`core/bus.ts` `emit`/`on`) and `App.svelte`'s central event wiring.

**No new backend command is needed.**

## Open decisions (settle before CP1)

| # | Question | Recommendation |
|---|---|---|
| S1 | Does the top-bar 🔍 icon open **spotlight** or stay on **SB search**? | Icon → **spotlight** (the fast finder people reach for). Keep `/brain` (and a spotlight result "Search the graph for …") for the graph view. |
| S2 | Keybind | **⌘K** (mac) / **Ctrl+K**. Free today (the Escape chain is the only global handler). Also close on `Esc`, toggle on re-press. |
| S3 | Sources in v1 | **All of:** workspace files, skills, routines, placeable modules, shell commands. The non-file sources filter in-memory (instant, free); only the file search is a round-trip. |
| S4 | Default action for a file result | **Enter → open in the centred viewer.** Secondary: **⌘Enter → reveal in Second Brain**. Skills → run; routines → open Routines board focused; modules → `/add <type>`; commands → run. |
| S5 | Empty query | A short "jump to" list: New note, Open Second Brain, Settings, + (later) recent files. |
| S6 | Result cap / grouping | ~20 rows, grouped by type with small headers, type order: commands → modules → skills → routines → files. Ranking: exact/prefix > word-boundary > substring; a file *content-only* hit ranks below any name hit. |

## Checkpoint 1 — `core/spotlight.ts` (pure model + tests)

- `type SpotlightItem = { id; kind: "command"|"module"|"skill"|"routine"|"file"; title; subtitle?; hint?; score; run: () => void | Promise<void> }` (the `run` closure is injected by the caller so the pure module never imports the bus / staging).
- `rankItems(query, sources) → SpotlightItem[]` — the scoring + sort + type-order + cap +
  group boundaries. `sources` is a plain record of the already-fetched raw lists
  (`commands`, `modules`, `skills`, `routines`, `fileHits`), so this is fully unit-testable.
- `scoreMatch(query, text) → number | null` — prefix / word-boundary / subsequence, `null`
  when no match. Case-insensitive, accent-insensitive where cheap.
- Tests: prefix beats substring, name hit beats content-only file hit, empty query returns
  the S5 suggestions, cap + grouping, a query matching nothing returns `[]`.

## Checkpoint 2 — `Spotlight.svelte` shell (instant sources)

- Centred overlay: backdrop (click-to-close), `role="dialog"` + `aria-modal`, focus trapped,
  one `<input>` autofocused, a results `<ul role="listbox">` with `aria-activedescendant`.
- Keyboard: ↑/↓ move the active row (wrap), Enter runs it, ⌘Enter runs its secondary action
  if any, Esc closes. Mouse hover also sets the active row.
- Wires the **instant** sources only: shell commands (`core/commands.ts`), placeable modules
  (`listModules()` minus already-placed singletons), skills, routines. `run` closures:
  command → `runCommand`; module → `createInstance(type)`; skill → `openStaged` skills-deck
  or `run_skill`; routine → `emit("open-routines" …)` or just open the board.
- `App.svelte`: mount `<Spotlight bind:open>`, a `window` `keydown` for ⌘K/Ctrl+K →
  `spotlightOpen = true`, and `on("shell:spotlight", …)`. Point `IconBar`'s 🔍 at
  `emit("shell:spotlight")` (S1) — and drop the `shell:search` → SB handler, or keep it
  behind `/brain` only.
- All `--ax-*` tokens; light + dark.

## Checkpoint 3 — async file search

- On input (debounced ~180 ms, and only for queries ≥ 2 chars) call
  `search_workspace(query, limit ~30)`; merge the hits through `rankItems` alongside the
  instant sources; show a subtle "searching…" affordance while in flight; ignore a stale
  response if the query moved on (request id / abort).
- File `run`: Enter → `openStaged("md-file", { path, mode: "read" }, "right")`; ⌘Enter →
  `emit("open-second-brain", { focus: \`file:${path}\` })`.
- `devmock.ts` already answers `search_workspace` from its fixture `files` map — nothing to
  add.

## Checkpoint 4 — polish, a11y, docs

- Focus returns to the previously-focused element on close. `Esc` doesn't leak to the
  Escape chain (StagingLayer / SB) — consume it.
- Doesn't fire ⌘K while a text input elsewhere has focus? (⌘K is safe; still, don't open a
  second one if already open.)
- `docs/architecture.md` §5 — add the spotlight to the shell description; note 🔍 = spotlight,
  `/brain` = graph search.
- `npm run check` + `vitest` (new `spotlight.test.ts`) green; browser pass via `agent-browser`.

## Suggested order

S1–S6 → **CP1** (pure model + tests) → **CP2** (overlay + instant sources) → **CP3** (file
search) → **CP4** (polish/a11y/docs). CP1 is pure logic; CP2–4 are frontend-only. No Rust.
