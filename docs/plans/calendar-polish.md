# Plan: Calendar module — mini-month, 7-day agenda, optional clock

Status: **in progress.** CP1–3 landed 2026-09-09; CP4–5 planned. Follows the owner's stepwise
workflow — confirm each checkpoint before starting the next.

**Owner sign-off (2026-09-09):** D1 = **Option (a)** (wide fetch + client-side filter — "find
ich klasse, verringert die Agentaufrufe"). D2/D4/D5/D6/D7/D8/D9 recommendations accepted as
written. Clock: sits **right of** the mini-month when enabled, its **height matches the
mini-month's**, and the user picks **analog or digital** in the options (D10, new).

## What the owner asked for (2026-09-09)

- A small **month calendar** top-left in the Calendar tool, current month. **Today** has a
  round highlight. Clicking a day **selects** it.
- Below it, the **agenda** shows the events of the **selected day plus the following week**
  (selected day … selected day + 7).
- The **`calendar-digest` skill** should fetch **today + 7** on an automated (Routine) run,
  but **selected day + 7** when the Calendar tile triggers it.
- Optionally a **clock** next to the calendar, **toggleable in the module options**.

## Current state (what we're changing)

- `modules/calendar.svelte` — an agenda list (`groupByDay` + `dayLabel`) over the last
  `calendar-digest` run's parsed JSON, a calendar-filter `<select>`, create/delete via
  one-shot instruct turns, a `↻` that calls `run_skill`, a mount-time refresh.
- `core/calendar.ts` — pure parse/filter/group logic + the write helpers. `CalendarDigest =
  { calendars: string[], events: CalendarEvent[] }`; `CalendarEvent.start`/`.end` are
  `YYYY-MM-DD` for all-day, ISO 8601 otherwise.
- `modules/calendar-settings.svelte` — only a `SkillNameField` today.
- `resources/calendar-digest/SKILL.md` — SOP fetches **"the next 14 days (today through
  +14)"**, `timeout_secs: 600`, `allowed_tools` the two `mcp__apple-reminders__calendar_*`
  tools. There is **no way to pass a date range to a skill run** today (`execute_skill(name,
  config)` runs the SOP body as the prompt, no params).
- Connector-module rule (`CLAUDE.md`, `docs/architecture.md` §5): "provider = skill, not
  code"; **every refresh is a real agent turn**; recurring refresh is a Routine's job only;
  writes go through a silent instruct turn. And since provider-hardening CP5 there is a
  **$2/day spend cap** — so an agent turn per calendar-day click is exactly the wrong shape.

## Open decisions (settle before CP1)

| # | Question | Recommendation |
|---|---|---|
| D1 | **How does "selected day + 7" reach the data?** **(a)** widen the skill to fetch a fixed ~2-month window once, filter to `selected…+7` **client-side** (free, instant, no new plumbing); **(b)** actually pass the selected date into the skill run each time (new `run_skill` param → prepended to the SOP prompt; costs an agent turn per selection outside cache, subject to the daily cap, ~15–60 s spinner per click). | **(a)** — matches the connector-module cost model. The skill still gains a wider, Routine-friendly default window (D5). |
| D2 | Week starts **Monday** or Sunday? | **Monday** (German convention). |
| D3 | Digital clock format | **`HH:mm`, 24-hour**, plus a weekday+date line under it. No per-second repaint (update every 20 s). |
| D4 | Clock default state? | **Off** — owner said "ein- oder ausschaltbar über die Optionen". |
| D10 | **Analog or digital clock** | Both, chosen in the options (`config.clockStyle: "digital" \| "analog"`, default `"digital"`). Analog = a small SVG face (hour/minute hand, optional slim second hand ticking every 20 s → jumps, acceptable; or omit the second hand). Enabled/style are two separate options: `showClock` (bool) + `clockStyle`. |
| D5 | New skill fetch window (Option a). | **"Today through the end of next month"** — always covers the current + next month for paging, and any 7-day agenda slice within it. Stated plainly in the SOP; no per-instance knob for v1. |
| D6 | Agenda rows for days in the 7-day range with **no** events? | **Collapse** — only days with events get a header (as today). If the whole 7-day window is empty, one "Keine Termine vom … bis …" line. |
| D7 | Paging the mini-calendar **beyond** the fetched window? | Allowed; days outside the window simply have no event dots, and a subtle "`↻` um diesen Zeitraum zu laden" hint appears. `↻` still just re-runs the skill (same window) — full far-future paging is explicitly out of scope for v1. |
| D8 | `selectedDay` persistence? | **Reset to today on mount** (a calendar opens on "now"); keep it in `config.selectedDay` only within the session isn't needed — just local `$state`. |
| D9 | Tile size | Bump `calendar` `defaultSize` to ~`{ w: 380, h: 460 }`, `minSize` ~`{ w: 300, h: 380 }` — the mini-month needs the height. |

If the owner is happy with all recommendations, CP3 is Option (a) and this plan needs no Rust
changes beyond the one SOP edit.

## Checkpoint 1 — mini-month widget (pure logic + component) — **done 2026-09-09**

- **`core/monthGrid.ts`** — `monthGrid(year, month, today?) → { year, month, label, weeks }`
  (`weeks` = 6×7 `DayCell { iso, inMonth, isToday }`, Monday-first); `isoDate`/`parseIso`/
  `todayIso` (all **local**, no UTC shift), `addDays` (DST-safe via `setDate`), `monthOf`,
  `shiftMonth`, `weekRange(iso, len=7)`, `weekdayLabels("short"|"narrow")`.
- **`modules/MiniCalendar.svelte`** — props `month {year,month}` / `selected` / `today` /
  `eventDays: Set<string>`, callbacks `onSelect(iso)` / `onPage(delta)`. `‹ Label ›` bar,
  weekday row, day buttons: `.today` = round `--ax-accent` fill + invert text; `.selected`
  = inset `--ax-accent` ring; `.out` = muted filler; a `--ax-accent` dot when
  `eventDays.has(iso)`. All `--ax-*` tokens.
- Tests: `core/monthGrid.test.ts` (10) — round-trips, `addDays` across month/year/DST,
  `monthOf`/`shiftMonth`, `weekRange`, grid is always 6×7 Monday-first, exactly-one-today,
  filler flags, label, `weekdayLabels`. `npm run check` + vitest (226) green.

## Checkpoint 2 — Calendar module layout rework — **done 2026-09-09**

- `calendar.svelte`: a `.top` flex row — `<MiniCalendar>` on the left (a `<!-- CP4 -->`
  placeholder marks where the clock mounts to its right). `selectedDay = $state(today)`,
  `viewMonth = $state(monthOf(today))`; `pickDay(iso)` sets `selectedDay` and pulls
  `viewMonth` along on a cross-month click, `pageMonth(delta)` shifts `viewMonth` only.
- Agenda = `groupByDay(filteredEvents.filter(e => weekRange(selectedDay).has(e.start.slice(0,10))))`
  with a `{dayLabel(selectedDay)} – {end}` caption; empty-window → "Keine Termine … in
  diesem Zeitraum." `eventDays` (a `Set` over all filtered events) feeds the grid dots.
- Kept: calendar filter, `+` create (its date defaults to `selectedDay`), last-run, `↻`,
  mount refresh. Tile `defaultSize` 380×540 / `minSize` 300×420; mini-month `width: 13rem`.
- `list` bridge action gained optional `from` / `days` (default today / 7) — the same
  client-side window filter, no re-run.
- `devmock.ts` calendar fixture now spans ~6 weeks (a couple past, several this week, two
  next month).
- **Verified** via `agent-browser` against `vite --port 1420`: mini-month renders (today
  ring, dots), day-select re-filters the agenda + caption, cross-month select pulls the grid
  along, prev/next paging leaves the selection alone, empty 7-day window shows the fallback.

## Checkpoint 3 — skill window (Option a per D1/D5) — **done 2026-09-09**

- `resources/calendar-digest/SKILL.md` step 2: "the next 14 days (today through +14)" →
  "from the **first day of the current month** through the **last day of next month**", with
  a one-line note that the module filters client-side. Frontmatter and the rest of the SOP
  unchanged. Copied to the live `~/.axiomata/skills/calendar-digest/SKILL.md` too (seed
  won't overwrite an existing file). Seed tests still pass (valid + parseable).
- No `execute_skill` / routine changes — the Routine run and the tile's `↻` fetch the same
  wide window; "today + 7" vs "selected + 7" is purely the client-side agenda filter.
- `docs/architecture.md` §5 has no 14-day wording to fix; the broader §5 connector-module
  note (mini-month + agenda slice) is folded into CP5.
- *(If the owner picks D1 (b) instead: separate mini-plan — `run_skill` gains
  `params: Record<string,string>` threaded into the prompt as a "Constraints:" preamble,
  `execute_skill`/`execute_prompt` signatures change, the routine target stays date-less
  (defaults today…+7 in the SOP), the module passes `from=<selectedDay>`. Costs one agent
  turn per out-of-cache selection — gate behind the CP5 spend guard, show the spinner.)*

## Checkpoint 4 — optional clock (digital / analog)

- **`modules/Clock.svelte`** — one component, `style: "digital" | "analog"` and `size`
  (px, = the mini-month's rendered height, passed from `calendar.svelte`) props.
  - Ticks: a single `setInterval` (20 s), `clearInterval` on destroy, recompute from
    `new Date()` each tick (drift-free, no accumulation).
  - **Digital**: `HH:mm` (24 h) large, weekday + `D. MMM YYYY` line under it (locale). Font
    size scales off `size`.
  - **Analog**: an inline `<svg viewBox="0 0 100 100">` face `size`×`size` — 12 tick marks,
    hour + minute hands rotated via `transform`, optional slim second hand (jumps every
    20 s — acceptable, or drop it). Face / hands / ticks all `--ax-*` tokens
    (`--ax-border`, `--ax-text`, `--ax-accent` for the hands). Legible in light + dark.
  - Purely presentational — no `ctx` needed; `calendar.svelte` owns the config read.
- **Options** (`calendar-settings.svelte`, `config.update` pattern like
  `todo-settings.svelte`):
  - `showClock: boolean` — a checkbox, default `false`.
  - `clockStyle: "digital" | "analog"` — a small radio / segmented control, default
    `"digital"`, only shown when `showClock` is on.
- `calendar.svelte` renders `<Clock style={clockStyle} size={miniMonthHeight} />` to the
  right of the mini-month only when `$config.showClock`. `miniMonthHeight` comes from a
  `bind:clientHeight` on the mini-month wrapper (or a shared fixed token if that's simpler).
- Config keys are free-form per-instance `config` entries (the canvas stores `config` as an
  opaque `Record<string, unknown>`), so **no `backend.ts` schema change** — read them
  defensively (`$config.clockStyle === "analog" ? "analog" : "digital"`).

## Checkpoint 5 — polish, tests, docs

- `npm run check` clean, `npx vitest run` green (new `monthGrid.test.ts`, updated
  `calendar`-related tests). `cargo` untouched unless D1 (b).
- Visual pass in `cargo tauri dev`: today ring, selected-day state, event dots, month
  paging, empty 7-day window, clock on/off, digital ↔ analog, clock height == mini-month
  height, tile at `minSize`.
- `docs/architecture.md` §5 "connector modules" — note the calendar tile now has a
  mini-month + 7-day agenda slice and the digest window is "this month + next".
- `ToDo.md`: this was not on the list; no entry to close.

## Suggested order

Decisions settled (D1–D10). **CP1** (widget) → **CP2** (layout) → **CP3** (SOP window) →
**CP4** (clock) → **CP5** (polish/docs). CP1–2 and CP4 are frontend-only; CP3 is a one-file
SOP edit (Option a) plus the live `~/.axiomata/skills/` copy.
