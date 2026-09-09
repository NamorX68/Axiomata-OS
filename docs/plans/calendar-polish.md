# Plan: Calendar module — mini-month, 7-day agenda, optional clock

Status: **COMPLETE** — CP1–5 landed 2026-09-09; CP6 follow-ups landed the same day.

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

## Checkpoint 4 — optional clock (digital / analog) — **done 2026-09-09**

- **`modules/Clock.svelte`** — `style: "digital" | "analog"` + `size` (px) props, one
  `setInterval(20 s)` recomputing `new Date()`, `clearInterval` on destroy.
  - **Digital**: `HH:mm` (24 h) + a `weekday, D. MMM YYYY` line, font sizes derived from
    `size` and capped (36 / 13 px) so a wide time string can't overflow the tile.
  - **Analog**: inline `<svg viewBox="0 0 100 100">` `size`×`size` — 12 ticks, white hour
    hand, `--ax-accent` minute hand, accent centre pin; all `--ax-*` tokens.
- **`calendar-settings.svelte`**: `showClock` checkbox (default off) + a `clockStyle`
  radio group (`digital` / `analog`, shown only when the clock is on), both via
  `config.update`.
- **`calendar.svelte`**: `bind:clientHeight` on a `.mini-wrap` → `clockSize =
  min(miniH, 128)` → `<Clock size={clockSize}>` right of the mini-month when
  `$config.showClock`. `.top` is `flex-wrap: wrap` so the clock drops below the grid in a
  narrow tile instead of overflowing. Config keys are free-form per-instance entries — no
  `backend.ts` change; read defensively.
- **Verified** in `agent-browser`: digital + analog both render, sized to the mini-month,
  toggle + style switch through settings.

## Checkpoint 5 — polish, tests, docs — **done 2026-09-09**

- `npm run check` clean, `npx vitest run` green (226; new `monthGrid.test.ts`).
- Visual pass done via `agent-browser` against `vite --port 1420` (the documented method for
  mock-backed checks): today ring, selected-day filter + caption, event dots, cross-month
  select, prev/next paging, empty 7-day window, clock on/off, digital ↔ analog, clock sized
  to the mini-month.
- `format.ts` `dayLabel` now judges "Today"/"Tomorrow" in **local** time (was UTC) and is
  DST-safe — otherwise the agenda header could disagree with `monthGrid`'s `todayIso` near
  midnight. `format.test.ts` still green.
- `docs/architecture.md` §5 connector-modules bullet rewritten (mini-month + selected-day
  agenda + wide digest window + optional clock); the stale "a `*-digest` skill (not in this
  repo)" corrected to the `resources/…/SKILL.md` seed. §7 post-M6 list updated.
- `ToDo.md`: not on the list; nothing to close.

## Checkpoint 6 — clamp paging + extract the create-form — **done 2026-09-09**

Owner asked for both after CP5.

- **Month paging is clamped to the fetched window** (this month + next). `core/monthGrid.ts`
  gains `monthDiff(a, b)`. `MiniCalendar.svelte` takes `minMonth` / `maxMonth`: the `‹` / `›`
  buttons `disabled` at the bounds, and day cells whose month is outside the range are
  `disabled` + dimmed. `calendar.svelte` passes `rangeMin = monthOf(today)` /
  `rangeMax = shiftMonth(rangeMin, 1)`, guards `pageMonth` / `pickDay`, and shows a
  `range-hint` line ("Nur dieser und der nächste Monat werden geladen — ↻ aktualisiert.")
  when the grid is on `rangeMax`. So the "empty December" confusion can't happen — you can't
  get there. (D7's full "load an arbitrary range" is still out of scope — it needs the
  declined D1(b) date-param plumbing.)
- **`CalendarCreateForm.svelte`** — the new-event form pulled out of `calendar.svelte`
  (mirrors `RoutineForm.svelte`): props `calendars` / `defaultCalendar` / `defaultDate` /
  `busy` / `error` / `onSubmit(NewCalendarEvent)` / `onCancel`; it owns the field state,
  the parent keeps `showCreate` / `creating` / `createError` and `handleCreate` (the
  instruct-turn write + local-digest patch). `calendar.svelte` down from ~525 to ~400
  lines; the `.create` / `.primary` CSS moved with the form.
- Browser-verified: paging stops at the bounds, out-of-range cells greyed, hint on the
  next-month view, create form still creates + closes. `monthDiff` unit-tested (227 total).

## Suggested order

Decisions settled (D1–D10). **CP1** (widget) → **CP2** (layout) → **CP3** (SOP window) →
**CP4** (clock) → **CP5** (polish/docs) → **CP6** (clamp + extract) — all landed 2026-09-09.
