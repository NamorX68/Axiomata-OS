# Plan: Spotlight search (⌘K)

Status: **decisions settled 2026-10-07 (grilled with the owner), not started.** It comes **before** the Orbit/2Brain merge
(`docs/plans/orbit-brain.md`), which only needs it for "search the graph". Follows the owner's stepwise workflow — confirm each
checkpoint before the next.

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

## Decisions (owner, 2026-10-07)

| # | Question | Decision |
|---|---|---|
| S1 | What does the top-bar 🔍 do? | It opens **spotlight**. The 2Brain's own search field is **removed entirely** (no more "matches bright, rest dimmed"): spotlight is the one search. |
| S2 | Keybind | **⌘K** / Ctrl+K, **global** (Orbit, Studio, Kanban). Free in the app code (checked 2026-10-07; the terminal and the editor are to be checked when building). Esc closes, a second ⌘K toggles. |
| S3 | Sources | Workspace files, skills, routines, placeable modules, shell commands — **plus Kanban cards, plans and Studio projects**. Non-file sources filter in memory; only the file search is a round-trip. |
| S4 | Look | A **pill in the middle of the screen** like macOS Spotlight, its size following the display (`--ax-ui-scale`). Hits are listed **below** the pill. |
| S5 | Behaviour | Typing searches. **↑/↓ only select**; nothing is opened until **Return or a double click**. |
| S6 | What Return does | Note/file: **open in the viewer**; **⌘Return: show in the 2Brain**. Area or skill node: show in the 2Brain. Card: open the Kanban at it. Plan or project: open the Studio there. Routine: the Routines board. Module or command: run it. "Search the graph" is an entry of its own. |
| S7 | Empty query, cap, grouping | As before: a short "jump to" list (New note, Settings, …); ~20 rows grouped by type (commands → modules → skills → routines → cards/plans/projects → files); exact/prefix > word-boundary > substring; a content-only file hit ranks below any name hit. |

## Checkpoint 1 — `core/spotlight.ts` (pure model + tests)

- `SpotlightItem = { id; kind; title; subtitle?; keywords?; contentMatches?; payload? }` — **built 2026-10-07**. The item carries an opaque
  `payload` instead of a `run` closure: CP2 switches on `kind`, so the pure module never imports the bus / staging. Per-kind cap of
  8 rows (`MAX_PER_KIND`) on top of the 20 in all, so 300 cards cannot push the other groups out.
- `rankItems(query, sources) → SpotlightItem[]` — the scoring + sort + type-order + cap +
  group boundaries. `sources` is a plain record of the already-fetched raw lists
  (`commands`, `modules`, `skills`, `routines`, `fileHits`), so this is fully unit-testable.
- `scoreMatch(query, text) → number | null` — prefix / word-boundary / subsequence, `null`
  when no match. Case-insensitive, accent-insensitive where cheap.
- Tests: prefix beats substring, name hit beats content-only file hit, empty query returns
  the S5 suggestions, cap + grouping, a query matching nothing returns `[]`.

## Checkpoint 2 — `Spotlight.svelte` shell (instant sources)

- Pill (S4): backdrop (click-to-close), `role="dialog"` + `aria-modal`, focus trapped, one `<input>` autofocused, the
  results `<ul role="listbox">` with `aria-activedescendant` below the pill.
- Keyboard: ↑/↓ move the active row (wrap), Return or a double click runs it, ⌘Return runs its secondary action if any,
  Esc closes. Mouse hover also sets the active row; nothing is run before Return/double click (S5).
- Wires the **instant** sources only: shell commands (`core/commands.ts`), placeable modules
  (`listModules()` minus already-placed singletons), skills, routines, Kanban cards, plans and Studio projects. `run` closures
  per S6: command → `runCommand`; module → `createInstance(type)`; skill → `openStaged` skills-deck or `run_skill`;
  routine → the Routines board; card → `openKanban` at the card; plan/project → the Studio.
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
- File `run`: Return → `openStaged("md-file", { path, mode: "read" }, "right")`; ⌘Enter →
  `emit("open-second-brain", { focus: \`file:${path}\` })`.
- `devmock.ts` already answers `search_workspace` from its fixture `files` map — nothing to
  add.

## Checkpoint 4 — polish, a11y, docs

- Focus returns to the previously-focused element on close. `Esc` doesn't leak to the
  Escape chain (StagingLayer / SB) — consume it.
- Doesn't fire ⌘K while a text input elsewhere has focus? (⌘K is safe; still, don't open a
  second one if already open.)
- `docs/architecture.md` §5 — add the spotlight to the shell description; note 🔍 = spotlight. Shortcut scheme: see the
  later UI/UX round (`docs/plans/orbit-brain.md`).
- `npm run check` + `vitest` (new `spotlight.test.ts`) green; browser pass via `agent-browser`.

## Suggested order

S1–S6 → **CP1** (pure model + tests) → **CP2** (overlay + instant sources) → **CP3** (file
search) → **CP4** (polish/a11y/docs). CP1 is pure logic; CP2–4 are frontend-only. No Rust.
