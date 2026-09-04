<!--
  calendar — an agenda list backed by the `calendar-digest` skill, not a live
  file: calendar data sits behind an MCP tool only an agent can reach, so
  every refresh is a real agent turn (unlike `todo`'s free 5 s file poll).
  This shell never triggers a run on a timer — it reads back whichever run
  happened most recently (`list_runs` + `get_run`), however it happened: by
  hand from the Skills Deck, on a schedule via a Routine, or the "Refresh"
  button here, which is just `run_skill` under the hood, same mechanism.

  Filter dropdown ("All calendars" + one entry per calendar the skill saw,
  even ones with no upcoming events) is a pure client-side filter over the
  last fetched digest — picking a calendar never re-runs the skill. Config
  (flip side): `calendar` — the last-selected filter, so it survives reload.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import type { RunRecord, RunSummary } from "../core/backend";
  import { CALENDAR_SKILL_NAME, EMPTY_DIGEST, eventTimeLabel, filterByCalendar, groupByDay, parseCalendarDigest, type CalendarDigest } from "../core/calendar";
  import { dayLabel, relativeTime } from "../core/format";
  import type { ModuleContext } from "../core/types";

  const RUN_LIMIT = 50;

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  let digest = $state<CalendarDigest>(EMPTY_DIGEST);
  let lastRun = $state<RunSummary | null>(null);
  let loading = $state(true);
  let running = $state(false);
  let error = $state("");
  let selectedCalendar = $state(typeof $config.calendar === "string" ? $config.calendar : "");

  const filteredEvents = $derived(filterByCalendar(digest.events, selectedCalendar === "" ? null : selectedCalendar));
  const groups = $derived(groupByDay(filteredEvents));

  function selectCalendar(e: Event) {
    selectedCalendar = (e.currentTarget as HTMLSelectElement).value;
    config.update((c) => ({ ...c, calendar: selectedCalendar }));
  }

  /** Applies a finished run (from either `loadLatest` or `refreshNow`) to
   *  the displayed digest, surfacing the skill's own failure/error text
   *  rather than a raw JSON-parse error where possible. */
  function applyRun(run: RunSummary, stdout: string | null) {
    lastRun = run;
    if (run.status === "failed" || stdout === null) {
      digest = EMPTY_DIGEST;
      error = run.error ?? "Last run failed.";
      return;
    }
    try {
      digest = parseCalendarDigest(stdout);
      error = "";
    } catch (err) {
      digest = EMPTY_DIGEST;
      error = String(err instanceof Error ? err.message : err);
    }
  }

  async function loadLatest() {
    loading = true;
    try {
      const runs = await ctx.invoke<RunSummary[]>("list_runs", { limit: RUN_LIMIT });
      const latest = runs.find((r) => r.skill_name === CALENDAR_SKILL_NAME) ?? null;
      if (!latest) {
        lastRun = null;
        digest = EMPTY_DIGEST;
        error = "";
        return;
      }
      const full = await ctx.invoke<RunRecord | null>("get_run", { id: latest.id });
      applyRun(latest, full?.stdout ?? null);
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
      const full = await ctx.invoke<RunRecord>("run_skill", { name: CALENDAR_SKILL_NAME });
      applyRun(full, full.stdout);
    } catch (err) {
      error = String(err);
    } finally {
      running = false;
    }
  }

  onMount(() => void loadLatest());
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
    <button type="button" class="refresh" title="Run calendar-digest now" aria-label="Refresh" disabled={running} onclick={() => void refreshNow()}>
      {running ? "…" : "↻"}
    </button>
  </div>

  {#if error}<p class="error">{error}</p>{/if}

  {#if loading}
    <p class="muted">Loading…</p>
  {:else if !lastRun}
    <p class="muted empty">
      No data yet — run <code>{CALENDAR_SKILL_NAME}</code> from the Skills Deck, schedule it as a
      Routine, or hit ↻ above.
    </p>
  {:else if groups.length === 0}
    <p class="muted empty">No upcoming events{selectedCalendar ? ` in "${selectedCalendar}"` : ""}.</p>
  {:else}
    <ul class="agenda">
      {#each groups as group (group.day)}
        <li class="day-group">
          <div class="day-head">{dayLabel(group.day)}</div>
          <ul class="events">
            {#each group.events as ev, i (i + ev.title + ev.start)}
              <li class="event">
                <span class="time muted">{eventTimeLabel(ev)}</span>
                <span class="title">{ev.title}</span>
                {#if ev.location}<span class="location muted">{ev.location}</span>{/if}
              </li>
            {/each}
          </ul>
        </li>
      {/each}
    </ul>
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
  .refresh {
    padding: 1px var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
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
