<!--
  The "new event" form for the `calendar` module, split out of
  `calendar.svelte` so that file stays focused on the digest / mini-month /
  agenda. Mirrors `RoutineForm.svelte`: this component only assembles the
  fields and calls `onSubmit` — the caller owns the instruct-turn write and
  the busy / error state.

  Mounted inside `{#if showCreate}` in the parent, so each open is a fresh
  instance and the `default*` props seed the fields once (never re-read).
-->
<script lang="ts">
  import type { NewCalendarEvent, RepeatFrequency } from "../core/calendar";
  import { tModules } from "../i18n/modules";

  /** The repeat choices; `label`, `every` and `unit` (counted by the interval) read the current language — `every`
   *  counts too, so an interval of 1 reads "Every day" / "Jeden Tag", not "Alle 1 Tag". */
  type Counted = (n: number) => string;
  const REPEATS: { id: RepeatFrequency | ""; label: () => string; every: Counted; unit: Counted }[] = [
    { id: "", label: () => tModules("repeatNone"), every: () => "", unit: () => "" },
    {
      id: "daily",
      label: () => tModules("repeatDaily"),
      every: (n) => tModules("everyDays", { n }),
      unit: (n) => tModules("unitDays", { n }),
    },
    {
      id: "weekly",
      label: () => tModules("repeatWeekly"),
      every: (n) => tModules("everyWeeks", { n }),
      unit: (n) => tModules("unitWeeks", { n }),
    },
    {
      id: "monthly",
      label: () => tModules("repeatMonthly"),
      every: (n) => tModules("everyMonths", { n }),
      unit: (n) => tModules("unitMonths", { n }),
    },
    {
      id: "yearly",
      label: () => tModules("repeatYearly"),
      every: (n) => tModules("everyYears", { n }),
      unit: (n) => tModules("unitYears", { n }),
    },
  ];

  let {
    calendars,
    defaultCalendar,
    defaultDate,
    busy = false,
    error = "",
    onSubmit,
    onCancel,
  }: {
    calendars: string[];
    /** Pre-selected calendar (the tile's current filter, or the first one). */
    defaultCalendar: string;
    /** Pre-filled date — the day selected in the mini-month. */
    defaultDate: string;
    busy?: boolean;
    error?: string;
    onSubmit: (input: NewCalendarEvent) => void;
    onCancel: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let title = $state("");
  // svelte-ignore state_referenced_locally
  let calendar = $state(defaultCalendar);
  // svelte-ignore state_referenced_locally
  let date = $state(defaultDate);
  let allDay = $state(false);
  let startTime = $state("");
  let endTime = $state("");
  let location = $state("");
  let repeat = $state<RepeatFrequency | "">("");
  let interval = $state(1);
  let until = $state("");

  const repeatChoice = $derived(REPEATS.find((r) => r.id === repeat));
  const repeatEvery = $derived(repeatChoice?.every(interval) ?? "");
  const repeatUnit = $derived(repeatChoice?.unit(interval) ?? "");
  const intervalOk = $derived(Number.isInteger(interval) && interval >= 1 && interval <= 99);
  const untilOk = $derived(until === "" || until >= date);
  const canSubmit = $derived(
    !busy && title.trim() !== "" && calendar !== "" && date !== "" && (repeat === "" || (intervalOk && untilOk)),
  );

  function submit() {
    if (!canSubmit) return;
    onSubmit({
      title: title.trim(),
      calendar,
      date,
      startTime: allDay ? null : startTime || null,
      endTime: allDay ? null : endTime || null,
      location: location.trim() || null,
      repeat: repeat === "" ? null : { frequency: repeat, interval, until: until || null },
    });
  }
</script>

<form class="create" onsubmit={(e) => (e.preventDefault(), submit())}>
  <input type="text" placeholder={tModules("titlePlaceholder")} bind:value={title} disabled={busy} />
  <div class="row">
    <select bind:value={calendar} disabled={busy} aria-label={tModules("calendar")}>
      {#each calendars as name (name)}
        <option value={name}>{name}</option>
      {/each}
    </select>
    <input type="date" bind:value={date} disabled={busy} />
  </div>
  <label class="row check"><input type="checkbox" bind:checked={allDay} disabled={busy} /> {tModules("allDay")}</label>
  {#if !allDay}
    <div class="row">
      <input type="time" bind:value={startTime} disabled={busy} aria-label={tModules("startTime")} />
      <input type="time" bind:value={endTime} disabled={busy} aria-label={tModules("endTime")} />
    </div>
  {/if}
  <input type="text" placeholder={tModules("locationPlaceholder")} bind:value={location} disabled={busy} />
  <select bind:value={repeat} disabled={busy} aria-label={tModules("repeat")}>
    {#each REPEATS as r (r.id)}
      <option value={r.id}>{r.label()}</option>
    {/each}
  </select>
  {#if repeat !== ""}
    <div class="row repeat">
      <label>
        {repeatEvery}
        <input type="number" min="1" max="99" bind:value={interval} disabled={busy} aria-label={tModules("interval")} />
        {repeatUnit}
      </label>
      <label>
        {tModules("until")}
        <input type="date" min={date} bind:value={until} disabled={busy} aria-label={tModules("repeatUntil")} />
      </label>
    </div>
    {#if !untilOk}<p class="error">{tModules("untilBeforeStart")}</p>{/if}
  {/if}
  {#if error}<p class="error">{error}</p>{/if}
  <div class="row">
    <button type="button" onclick={onCancel} disabled={busy}>{tModules("cancel")}</button>
    <button type="submit" class="primary" disabled={!canSubmit}>
      {busy ? tModules("creating") : tModules("create")}
    </button>
  </div>
</form>

<style>
  .create {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    flex: 0 0 auto;
    padding: var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
  }
  .row {
    display: flex;
    gap: var(--ax-space-2);
  }
  .row > * {
    flex: 1 1 0;
    min-width: 0;
  }
  .row.repeat label {
    display: flex;
    align-items: center;
    gap: var(--ax-space-1);
    font-size: var(--ax-font-size-sm);
  }
  .row.repeat input[type="number"] {
    width: calc(56px * var(--ax-ui-scale));
    flex: 0 0 auto;
  }
  .row.repeat input[type="date"] {
    flex: 1 1 auto;
    min-width: 0;
  }
  .row.check {
    flex: 0 0 auto;
    align-items: center;
    font-size: var(--ax-font-size-sm);
  }
  .row.check input {
    flex: 0 0 auto;
    accent-color: var(--ax-accent);
  }
  input[type="text"] {
    width: 100%;
  }
  .row:last-child {
    justify-content: flex-end;
  }
  .primary {
    background: var(--ax-accent);
    border-color: var(--ax-accent);
    color: var(--ax-text-invert);
    font-weight: 600;
  }
  .primary:hover:not(:disabled) {
    background: var(--ax-accent-hover);
  }
  .error {
    color: var(--ax-danger);
    font-size: var(--ax-font-size-sm);
    margin: 0;
  }
</style>
