# Plan: Engines, roles and agents made clear (Studio, package A) and a better Flow graph (package B)

Status: **decisions settled 2026-10-11 (grilled with the owner: "so wie du vorschlägst", plus Q6: the graph's nodes get a nicer tile
look). Package A (R-1…R-5) built the same day, checked in the browser mock, not yet on the Mac; package B not started.** Package A is built first, B after it, each as its own commit. Reviews run once before each commit (lean cadence, `CLAUDE.md`).

## Why

The owner finds the area of engines, roles and agents "still very confusing": the inspector's narrow "Engines & roles" tab lists
engines and roles one under the other, and the form of a new or edited entry opens at the *end* of the long list, out of sight; an
agent has to be assembled by hand in the left panel from a name, an engine and a role (although a role already brings its engine);
an agent's card does not say what kind of agent it is (the screenshot showed "Claude Code · claude-haiku-5-5 · reviewer" cut off);
and a role's name has to be typed as a slug. The Flow's graph is "very plain".

## Decisions (owner, 2026-10-11)

| # | Question | Decision |
|---|---|---|
| R-1 | Where engines and roles live | **One large pane in the dock** (pane kind `roster`, one per layout, "Engines & roles"), opened from the Agents panel's "Manage…". Master and detail **side by side**: the list on the left (a switch *Engines \| Roles*), the form on the right — never under the list. The inspector's "Engines & roles" tab is removed. |
| R-2 | What the words mean | Three things named apart everywhere: **engine** = what thinks (harness + model), **role** = what it is for (kind of work, instructions, limits), **agent** = a session in the project (a role on an engine, with its own branch and worktree). One line in the pane says so. |
| R-3 | Making agents | **On demand, not on stock.** The Agents panel offers each role of the project as a "Start" chip: it opens the newest agent of that role on the role's engine that has no pane yet, or makes one. A role without a default engine (the planner) opens the form with the role chosen. "New agent…" stays for something quite different. |
| R-4 | Kind of agent on the card | A **role chip** (tinted by the role's kind: plan, review, implement, other), the **engine's name** instead of the model id (an agent without an engine falls back to harness + short model), and the avatar shows the **harness** ("CC" / "OC"). |
| R-5 | Ids | **Derived.** A role's file name is a slug; the form takes a free name ("Reviewer light"), shows the id it becomes (`reviewer-light`) and appends `-2`, `-3` on a clash; an empty name becomes `<kind>-<tier>`. An engine's id comes from its label (empty: harness + model). Id and the rarely used fields sit under "Advanced"; an existing entry's id stays fixed. |
| R-6 | Order | Package **A** (R-1…R-5), then package **B** (the graph). |
| R-7 | The graph (package B) | The nodes become **tiles**: a state-coloured head with the card number and kind, a title that may take two lines, and a foot with role/engine and the state; the three Flow panes get room (the planning column never narrower than readable). Details are settled when B starts. |

## Checkpoints (package A)

1. **A1 — logic** (`ide/rosterLook.ts` + tests): `slugify`, `uniqueSlug`, `engineIdFor`, `roleIdFor`, `modelShort`,
   `engineName`, `harnessBadge`, `roleTone`, `agentNameFor`, `startableAgent`.
2. **A2 — the pane:** `RosterPane` (tabs Engines | Roles), the two sections reshaped to list + form side by side, ids derived,
   "Advanced"; pane kind, `PaneHost` branch, opener in `IdeView`; the inspector tab removed.
3. **A3 — the Agents panel:** the explaining line, "Start" chips per role, the card with role chip, engine name and harness badge.
4. **A4 — docs:** this plan's status, `docs/architecture.md`, the memory note.

## Not in scope

Changing the roster's file format (`axiomata-roster`, Rust) — a role's `name` stays its id and its file name; a separate display
name would be a migration. Agents on stock. Moving engines/roles back into the app settings.
