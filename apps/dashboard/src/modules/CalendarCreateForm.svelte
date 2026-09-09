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
  import type { NewCalendarEvent } from "../core/calendar";

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

  const canSubmit = $derived(!busy && title.trim() !== "" && calendar !== "" && date !== "");

  function submit() {
    if (!canSubmit) return;
    onSubmit({
      title: title.trim(),
      calendar,
      date,
      startTime: allDay ? null : startTime || null,
      endTime: allDay ? null : endTime || null,
      location: location.trim() || null,
    });
  }
</script>

<form class="create" onsubmit={(e) => (e.preventDefault(), submit())}>
  <input type="text" placeholder="Title…" bind:value={title} disabled={busy} />
  <div class="row">
    <select bind:value={calendar} disabled={busy} aria-label="Calendar">
      {#each calendars as name (name)}
        <option value={name}>{name}</option>
      {/each}
    </select>
    <input type="date" bind:value={date} disabled={busy} />
  </div>
  <label class="row check"><input type="checkbox" bind:checked={allDay} disabled={busy} /> All day</label>
  {#if !allDay}
    <div class="row">
      <input type="time" bind:value={startTime} disabled={busy} aria-label="Start time" />
      <input type="time" bind:value={endTime} disabled={busy} aria-label="End time" />
    </div>
  {/if}
  <input type="text" placeholder="Location (optional)…" bind:value={location} disabled={busy} />
  {#if error}<p class="error">{error}</p>{/if}
  <div class="row">
    <button type="button" onclick={onCancel} disabled={busy}>Cancel</button>
    <button type="submit" class="primary" disabled={!canSubmit}>
      {busy ? "Creating…" : "Create"}
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
