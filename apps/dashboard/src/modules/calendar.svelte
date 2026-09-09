<!--
  calendar — an agenda list backed by the `calendar-digest` skill (or
  whatever this instance's settings face has renamed it to, see
  `resolveSkillName`), not a live file: calendar data sits behind an MCP
  tool only an agent can reach, so every refresh is a real agent turn
  (unlike `todo`'s free 5 s file poll). This shell never triggers a run on
  a repeating timer — recurring refresh is exclusively a Routine's job —
  but it does trigger one real run of its own the moment it first mounts
  (app start, or right after it's newly placed via Add Module), same as
  `mail`/`reminders`. Otherwise it just reads back whichever run happened
  most recently (`list_runs` + `get_run`), however it happened: by hand
  from the Skills Deck, on a schedule via a Routine, or the "Refresh"
  button here, which is just `run_skill` under the hood, same mechanism.

  Filter dropdown ("All calendars" + one entry per calendar the skill saw,
  even ones with no upcoming events) is a pure client-side filter over the
  last fetched digest — picking a calendar never re-runs the skill. Config
  (flip side): `calendar` — the last-selected filter, so it survives reload.

  Create / delete go through a one-shot instruct turn (`core/instruct.ts`),
  not the digest skill — a write needs per-call parameters a skill can't
  take. Create asks the agent to report the new event's id back and inserts
  it into the local digest directly (no full re-run); delete removes the
  deleted id from the local digest the same way. Both are therefore only as
  fresh as the last read — a stray edit made outside the app (or by another
  device) won't show until the next ↻.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import type { RunRecord, RunSummary } from "../core/backend";
  import {
    CALENDAR_SKILL_NAME,
    createCalendarEvent,
    deleteCalendarEvent,
    EMPTY_DIGEST,
    eventTimeLabel,
    filterByCalendar,
    loadLatestCalendarDigest,
    parseCalendarDigest,
    type CalendarDigest,
    type CalendarEvent,
    type NewCalendarEvent,
  } from "../core/calendar";
  import { dayLabel, relativeTime } from "../core/format";
  import { monthDiff, monthOf, shiftMonth, todayIso, weekRange, type YearMonth } from "../core/monthGrid";
  import { resolveSkillName } from "../core/skillRun";
  import type { ModuleContext } from "../core/types";
  import CalendarCreateForm from "./CalendarCreateForm.svelte";
  import Clock from "./Clock.svelte";
  import MiniCalendar from "./MiniCalendar.svelte";

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  const skillName = $derived(resolveSkillName($config, CALENDAR_SKILL_NAME));

  let digest = $state<CalendarDigest>(EMPTY_DIGEST);
  let lastRun = $state<RunSummary | null>(null);
  let loading = $state(true);
  let running = $state(false);
  let error = $state("");
  let selectedCalendar = $state(typeof $config.calendar === "string" ? $config.calendar : "");

  const filteredEvents = $derived(filterByCalendar(digest.events, selectedCalendar === "" ? null : selectedCalendar));

  // --- Mini-month + 7-day agenda window ---
  // `selectedDay` starts on "today" every mount (a calendar opens on now);
  // `viewMonth` is the month the grid shows and can be paged independently.
  const today = todayIso();
  let selectedDay = $state(today);
  let viewMonth = $state<YearMonth>(monthOf(today));

  // `calendar-digest` fetches this month + next month only, so the mini-month
  // can't be paged (or a day picked) outside that range — there'd be no data
  // and it would read as "nothing scheduled".
  const rangeMin = monthOf(today);
  const rangeMax = shiftMonth(rangeMin, 1);
  const inRange = (m: YearMonth) => monthDiff(rangeMin, m) >= 0 && monthDiff(m, rangeMax) >= 0;
  const atRangeEnd = $derived(monthDiff(viewMonth, rangeMax) === 0);

  /** How many days the agenda lists from `selectedDay` on — every day gets a
   *  header, even empty ones. Configurable in settings; default 5. */
  const agendaSpan = $derived(
    Math.min(14, Math.max(1, typeof $config.agendaDays === "number" ? Math.floor($config.agendaDays) : 5)),
  );
  /** The ISO days the agenda shows, `selectedDay … +span-1`. */
  const agendaDayList = $derived(weekRange(selectedDay, agendaSpan));
  /** Events (already calendar-filtered) grouped by their day, for lookup. */
  const eventsByDay = $derived(
    filteredEvents.reduce((map, e) => {
      const d = e.start.slice(0, 10);
      (map.get(d) ?? map.set(d, []).get(d)!).push(e);
      return map;
    }, new Map<string, CalendarEvent[]>()),
  );
  /** True once the whole span has zero events — shows a single empty line. */
  const agendaEmpty = $derived(agendaDayList.every((d) => !eventsByDay.has(d)));
  /** Days (any month) that have at least one event, for the grid's dots. */
  const eventDays = $derived(new Set(filteredEvents.map((e) => e.start.slice(0, 10))));

  const agendaEnd = $derived(agendaDayList[agendaDayList.length - 1]);
  const agendaCaption = $derived(
    `${dayLabel(selectedDay)} – ${new Date(`${agendaEnd}T00:00:00`).toLocaleDateString(undefined, {
      day: "numeric",
      month: "short",
    })}`,
  );

  function pickDay(iso: string) {
    const m = monthOf(iso);
    if (!inRange(m)) return; // out-of-range cells are disabled; guard anyway
    selectedDay = iso;
    if (m.year !== viewMonth.year || m.month !== viewMonth.month) viewMonth = m;
  }
  function pageMonth(delta: number) {
    const next = shiftMonth(viewMonth, delta);
    if (inRange(next)) viewMonth = next;
  }

  // Optional clock (right of the mini-month) — toggled + styled in settings.
  // `miniH` is the mini-month's rendered height so the clock lines up with it.
  let miniH = $state(0);
  const showClock = $derived($config.showClock === true);
  const clockStyle = $derived($config.clockStyle === "analog" ? "analog" : "digital");
  // Track the mini-month height, but cap it so the clock still fits beside the
  // 11rem grid in a default-width tile (the row wraps below that).
  const clockSize = $derived(Math.min(miniH, 160));

  function selectCalendar(e: Event) {
    selectedCalendar = (e.currentTarget as HTMLSelectElement).value;
    config.update((c) => ({ ...c, calendar: selectedCalendar }));
  }

  // --- Create --- (fields + form markup live in `CalendarCreateForm.svelte`)
  let showCreate = $state(false);
  let creating = $state(false);
  let createError = $state("");

  function openCreate() {
    createError = "";
    showCreate = true;
  }

  async function handleCreate(input: NewCalendarEvent) {
    if (creating) return;
    creating = true;
    createError = "";
    try {
      const event = await createCalendarEvent(ctx.invoke, input);
      digest = { ...digest, events: [...digest.events, event].sort((a, b) => a.start.localeCompare(b.start)) };
      showCreate = false;
    } catch (err) {
      createError = String(err instanceof Error ? err.message : err);
    } finally {
      creating = false;
    }
  }

  // --- Delete ---
  let deletingId = $state<string | null>(null);

  async function removeEvent(ev: CalendarEvent) {
    if (deletingId) return;
    deletingId = ev.id;
    try {
      await deleteCalendarEvent(ctx.invoke, ev.id);
      digest = { ...digest, events: digest.events.filter((e) => e.id !== ev.id) };
    } catch (err) {
      error = String(err instanceof Error ? err.message : err);
    } finally {
      deletingId = null;
    }
  }

  /** Applies a just-finished `run_skill` result (`refreshNow` only —
   *  `loadLatest` goes through `loadLatestCalendarDigest` instead, which
   *  already does this same success/failure/parse-error mapping for the
   *  "find the latest run" path). */
  function applyFreshRun(run: RunRecord) {
    lastRun = run;
    if (run.status === "failed") {
      digest = EMPTY_DIGEST;
      error = run.error ?? "Last run failed.";
      return;
    }
    try {
      digest = parseCalendarDigest(run.stdout);
      error = "";
    } catch (err) {
      digest = EMPTY_DIGEST;
      error = String(err instanceof Error ? err.message : err);
    }
  }

  async function loadLatest() {
    loading = true;
    try {
      const result = await loadLatestCalendarDigest(ctx.invoke, skillName);
      lastRun = result.run;
      digest = result.digest;
      error = result.error ?? "";
    } catch (err) {
      error = String(err);
    } finally {
      loading = false;
    }
  }

  async function refreshNow() {
    if (running) return;
    running = true;
    try {
      const full = await ctx.invoke<RunRecord>("run_skill", { name: skillName });
      applyFreshRun(full);
    } catch (err) {
      error = String(err);
    } finally {
      running = false;
    }
  }

  // Show whatever's cached immediately (fast), then kick off one real run
  // in the background — the mount-time refresh this module's doc comment
  // describes. `refreshNow` already no-ops if a run is somehow already in
  // flight, so this can't double-fire.
  onMount(() => void loadLatest().then(refreshNow));
</script>

<div class="calendar">
  <div class="head">
    <select value={selectedCalendar} onchange={selectCalendar} aria-label="Filter by calendar">
      <option value="">All calendars</option>
      {#each digest.calendars as name (name)}
        <option value={name}>{name}</option>
      {/each}
    </select>
    <span class="spacer"></span>
    <span class="muted last-run" title={lastRun ? `Last run: ${lastRun.started_at}` : "No run yet"}>
      {lastRun ? relativeTime(lastRun.started_at) : "never run"}
    </span>
    <button type="button" class="add" title="New event" aria-label="New event" onclick={openCreate}>+</button>
    <button type="button" class="refresh" title="Run calendar-digest now" aria-label="Refresh" disabled={running} onclick={() => void refreshNow()}>
      {running ? "…" : "↻"}
    </button>
  </div>

  <div class="top">
    <div class="mini-wrap" bind:clientHeight={miniH}>
      <MiniCalendar
        month={viewMonth}
        selected={selectedDay}
        {today}
        {eventDays}
        minMonth={rangeMin}
        maxMonth={rangeMax}
        onSelect={pickDay}
        onPage={pageMonth}
      />
    </div>
    {#if showClock && clockSize > 0}
      <Clock style={clockStyle} size={clockSize} />
    {/if}
  </div>

  {#if atRangeEnd}
    <p class="range-hint muted">Nur dieser und der nächste Monat werden geladen — ↻ aktualisiert.</p>
  {/if}

  {#if showCreate}
    <CalendarCreateForm
      calendars={digest.calendars}
      defaultCalendar={selectedCalendar || digest.calendars[0] || ""}
      defaultDate={selectedDay}
      busy={creating}
      error={createError}
      onSubmit={handleCreate}
      onCancel={() => (showCreate = false)}
    />
  {/if}

  {#if error}<p class="error">{error}</p>{/if}

  {#if loading}
    <p class="muted">Loading…</p>
  {:else if !lastRun && running}
    <p class="muted empty">Running <code>{skillName}</code> for the first time…</p>
  {:else if !lastRun}
    <p class="muted empty">
      No data yet — run <code>{skillName}</code> from the Skills Deck, schedule it as a
      Routine, or hit ↻ above.
    </p>
  {:else}
    <div class="agenda-caption muted">{agendaCaption}</div>
    {#if agendaEmpty}
      <p class="muted empty">
        Keine Termine {selectedCalendar ? `in „${selectedCalendar}" ` : ""}in diesem Zeitraum.
      </p>
    {:else}
      <ul class="agenda">
        {#each agendaDayList as day (day)}
          {@const dayEvents = eventsByDay.get(day) ?? []}
          <li class="day-group">
            <div class="day-head">{dayLabel(day)}</div>
            {#if dayEvents.length === 0}
              <p class="muted no-events">–</p>
            {:else}
              <ul class="events">
                {#each dayEvents as ev (ev.id)}
                  <li class="event">
                    <span class="time muted">{eventTimeLabel(ev)}</span>
                    <span class="title">{ev.title}</span>
                    {#if ev.location}<span class="location muted">{ev.location}</span>{/if}
                    <button
                      type="button"
                      class="del"
                      aria-label={`Delete "${ev.title}"`}
                      disabled={deletingId === ev.id}
                      onclick={() => void removeEvent(ev)}
                    >
                      {deletingId === ev.id ? "…" : "✕"}
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</div>

<style>
  .calendar {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    padding: var(--ax-space-3);
    gap: var(--ax-space-2);
  }

  .head {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    flex: 0 0 auto;
  }
  select {
    min-width: 0;
    max-width: 55%;
  }
  .spacer {
    flex: 1 1 auto;
  }
  .last-run {
    white-space: nowrap;
  }
  .add,
  .refresh {
    padding: 1px var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }

  .top {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--ax-space-2);
    flex: 0 0 auto;
  }
  .mini-wrap {
    flex: 0 0 auto;
  }
  .mini-wrap :global(.mini) {
    /* Compact so the agenda below still has room and the (enlarged) clock
       still fits beside it on a default-width tile; the mini-month drives
       the row height and the clock matches it (`size` prop). */
    width: 11rem;
    max-width: 100%;
  }
  /* Push the optional clock to the tile's right edge, away from the grid. */
  .top :global(.clock) {
    margin-left: auto;
  }
  .mini-wrap :global(.mini .day) {
    font-size: var(--ax-font-size-xs);
  }

  .agenda-caption {
    flex: 0 0 auto;
    font-size: var(--ax-font-size-sm);
    font-weight: 600;
    padding-top: var(--ax-space-1);
  }
  .range-hint {
    flex: 0 0 auto;
    font-size: var(--ax-font-size-xs);
  }

  .agenda {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    min-height: 0;
    flex: 1 1 auto;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
  }
  .day-head {
    font-size: var(--ax-font-size-sm);
    font-weight: 600;
    color: var(--ax-accent);
    text-transform: uppercase;
    letter-spacing: 0.02em;
    padding-bottom: var(--ax-space-1);
  }
  .events {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
  }
  .no-events {
    padding: 0 0 var(--ax-space-1);
    opacity: 0.7;
  }
  .event {
    display: flex;
    align-items: baseline;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) 0;
    border-top: 1px solid var(--ax-border);
  }
  .event:first-child {
    border-top: none;
  }
  .time {
    flex: 0 0 auto;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    white-space: nowrap;
  }
  .title {
    flex: 1 1 auto;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .location {
    flex: 0 1 auto;
    font-size: var(--ax-font-size-sm);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .del {
    flex: 0 0 auto;
    padding: 0 var(--ax-space-1);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
    opacity: 0;
  }
  .event:hover .del,
  .del:focus-visible {
    opacity: 1;
  }

  .muted {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
  .empty {
    padding: var(--ax-space-2) 0;
  }
  .error {
    color: var(--ax-danger);
    font-size: var(--ax-font-size-sm);
    margin: 0;
  }
  p {
    margin: 0;
  }
  code {
    font-family: var(--ax-font-mono);
  }
</style>
