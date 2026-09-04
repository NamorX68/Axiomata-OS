<!--
  reminders — a single-list task view backed by the `reminders-digest`
  skill. Same no-live-poll shape as `calendar` (reminders data sits behind
  an MCP tool only an agent can reach, so every refresh is a real agent
  turn, never a timer) — this shell reads back whichever run happened most
  recently, however it happened: Skills Deck, a Routine, or this tile's own
  ↻ (a plain `run_skill` call under the hood, same mechanism).

  Unlike `calendar`, there is no "all lists" option — the owner's Apple
  Reminders lists have no shared theme (shopping lists, projects, gift
  ideas, …), so the picker always shows exactly one real list, defaulting
  to the alphabetically first one until something else is chosen. Config
  (flip side): `list` — the last-selected list, so it survives reload.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import type { RunRecord, RunSummary } from "../core/backend";
  import { relativeTime } from "../core/format";
  import {
    defaultList,
    EMPTY_REMINDER_DIGEST,
    loadLatestReminderDigest,
    parseReminderDigest,
    REMINDERS_SKILL_NAME,
    tasksForList,
    type ReminderDigest,
  } from "../core/reminders";
  import type { ModuleContext } from "../core/types";

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  let digest = $state<ReminderDigest>(EMPTY_REMINDER_DIGEST);
  let lastRun = $state<RunSummary | null>(null);
  let loading = $state(true);
  let running = $state(false);
  let error = $state("");
  let selectedList = $state(typeof $config.list === "string" ? $config.list : "");

  const tasks = $derived(selectedList ? tasksForList(digest.tasks, selectedList) : []);

  function selectList(e: Event) {
    selectedList = (e.currentTarget as HTMLSelectElement).value;
    config.update((c) => ({ ...c, list: selectedList }));
  }

  /** Keeps the current selection if it still exists in a freshly loaded
   *  digest, otherwise falls back to the alphabetically first list — there
   *  is no "all lists" fallback to fall back to. */
  function settleSelection() {
    if (selectedList && digest.lists.includes(selectedList)) return;
    selectedList = defaultList(digest.lists) ?? "";
  }

  /** Applies a just-finished `run_skill` result (`refreshNow` only —
   *  `loadLatest` goes through `loadLatestReminderDigest` instead, which
   *  already does this same mapping for the "find the latest run" path). */
  function applyFreshRun(run: RunRecord) {
    lastRun = run;
    if (run.status === "failed") {
      digest = EMPTY_REMINDER_DIGEST;
      error = run.error ?? "Last run failed.";
    } else {
      try {
        digest = parseReminderDigest(run.stdout);
        error = "";
      } catch (err) {
        digest = EMPTY_REMINDER_DIGEST;
        error = String(err instanceof Error ? err.message : err);
      }
    }
    settleSelection();
  }

  async function loadLatest() {
    loading = true;
    try {
      const result = await loadLatestReminderDigest(ctx.invoke);
      lastRun = result.run;
      digest = result.digest;
      error = result.error ?? "";
      settleSelection();
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
      const full = await ctx.invoke<RunRecord>("run_skill", { name: REMINDERS_SKILL_NAME });
      applyFreshRun(full);
    } catch (err) {
      error = String(err);
    } finally {
      running = false;
    }
  }

  onMount(() => void loadLatest());
</script>

<div class="reminders">
  <div class="head">
    <select value={selectedList} onchange={selectList} disabled={digest.lists.length === 0} aria-label="Reminder list">
      {#if digest.lists.length === 0}
        <option value="">No lists</option>
      {/if}
      {#each digest.lists as name (name)}
        <option value={name}>{name}</option>
      {/each}
    </select>
    <span class="spacer"></span>
    <span class="muted last-run" title={lastRun ? `Last run: ${lastRun.started_at}` : "No run yet"}>
      {lastRun ? relativeTime(lastRun.started_at) : "never run"}
    </span>
    <button type="button" class="refresh" title="Run reminders-digest now" aria-label="Refresh" disabled={running} onclick={() => void refreshNow()}>
      {running ? "…" : "↻"}
    </button>
  </div>

  {#if error}<p class="error">{error}</p>{/if}

  {#if loading}
    <p class="muted">Loading…</p>
  {:else if !lastRun}
    <p class="muted empty">
      No data yet — run <code>{REMINDERS_SKILL_NAME}</code> from the Skills Deck, schedule it as a
      Routine, or hit ↻ above.
    </p>
  {:else if digest.lists.length === 0}
    <p class="muted empty">No reminder lists found.</p>
  {:else if tasks.length === 0}
    <p class="muted empty">Nothing open in &quot;{selectedList}&quot;.</p>
  {:else}
    <ul class="tasks">
      {#each tasks as t, i (i + t.title + t.list)}
        <li class="task">
          {#if t.priority !== "none"}
            <span class="prio prio-{t.priority}" title={`Priority: ${t.priority}`} aria-hidden="true"></span>
          {/if}
          <span class="title">{t.title}</span>
          {#if t.dueDate}<span class="due muted">{t.dueDate.slice(0, 10)}</span>{/if}
        </li>
        {#if t.notes}<li class="notes muted">{t.notes}</li>{/if}
      {/each}
    </ul>
  {/if}
</div>

<style>
  .reminders {
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

  .tasks {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    min-height: 0;
    flex: 1 1 auto;
  }
  .task {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) 0;
    border-top: 1px solid var(--ax-border);
  }
  .task:first-child {
    border-top: none;
  }
  .prio {
    flex: 0 0 auto;
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  .prio-high {
    background: var(--ax-danger);
  }
  .prio-medium {
    background: var(--ax-warning);
  }
  .prio-low {
    background: var(--ax-text-muted);
  }
  .title {
    flex: 1 1 auto;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .due {
    flex: 0 0 auto;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    white-space: nowrap;
  }
  .notes {
    padding: 0 0 var(--ax-space-1) var(--ax-space-4);
    font-size: var(--ax-font-size-sm);
    overflow-wrap: anywhere;
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
