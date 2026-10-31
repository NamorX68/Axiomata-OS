<!--
  memory-status — the M2 router at a glance: workspace, tracked files, last
  sync, fresh/stale badge, and a Sync button. Polls every POLL_MS while
  mounted. `config.compact` (flip side) collapses it to a single badge line.
-->
<script lang="ts">
  import { messageOf } from "../core/errors";
  import { onMount } from "svelte";

  import type { MemoryStatus, SyncReport } from "../core/backend";
  import { relativeTime, shortPath } from "../core/format";
  import type { ModuleContext } from "../core/types";
  import { tBrain } from "../i18n/brain";

  const POLL_MS = 3000;

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  let status = $state<MemoryStatus | null>(null);
  let error = $state("");
  let syncing = $state(false);
  /** The last sync's outcome, kept as values so its line follows a language switch. */
  type Report =
    | { ok: true; written: number; unchanged: number; tracked: number; failed: number }
    | { ok: false; error: string };
  let lastReport = $state<Report | null>(null);
  const reportText = $derived.by(() => {
    const r = lastReport;
    if (!r) return "";
    if (!r.ok) return tBrain("syncFailed", { error: r.error });
    const base =
      r.written === 0
        ? tBrain("inSync", { files: tBrain("files", { n: r.tracked }) })
        : tBrain("wrote", { n: r.written, unchanged: r.unchanged });
    return r.failed === 0 ? base : `${base} · ${tBrain("failedCount", { n: r.failed })}`;
  });

  const compact = $derived($config.compact === true);

  async function refresh() {
    try {
      status = await ctx.invoke<MemoryStatus>("get_memory_status");
      error = "";
    } catch (err) {
      error = messageOf(err);
    }
  }

  async function sync() {
    if (syncing) return;
    syncing = true;
    try {
      const r = await ctx.invoke<SyncReport>("sync_memory");
      lastReport = {
        ok: true,
        written: r.written.length,
        unchanged: r.unchanged,
        tracked: r.tracked_files,
        failed: r.failed.length,
      };
    } catch (err) {
      lastReport = { ok: false, error: messageOf(err) };
    } finally {
      syncing = false;
      await refresh();
    }
  }

  onMount(() => {
    void refresh();
    const id = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(id);
  });
</script>

<div class="mem" class:compact>
  {#if error}
    <p class="error">{error}</p>
  {:else if !status}
    <p class="muted">{tBrain("loading")}</p>
  {:else}
    <div class="row">
      <span class="badge" class:stale={status.stale} class:fresh={!status.stale}>
        {status.stale ? tBrain("stale") : tBrain("fresh")}
      </span>
      <span class="files">
        <strong>{status.tracked_files}</strong> {tBrain("filesWord", { n: status.tracked_files })}
      </span>
      <span class="muted">{tBrain("synced", { when: relativeTime(status.last_sync) })}</span>
      <button type="button" class="sync" disabled={syncing} onclick={sync}>
        {syncing ? tBrain("syncing") : tBrain("sync")}
      </button>
    </div>
    {#if !compact}
      <p class="path" title={status.workspace_root}>{shortPath(status.workspace_root, 3)}</p>
      {#if reportText}<p class="muted report">{reportText}</p>{/if}
    {/if}
  {/if}
</div>

<style>
  .mem {
    padding: var(--ax-space-3);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
  }
  .mem.compact {
    padding: var(--ax-space-2) var(--ax-space-3);
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--ax-space-3);
    flex-wrap: wrap;
  }
  .sync {
    margin-left: auto;
  }

  .badge {
    padding: 2px var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    font-size: var(--ax-font-size-sm);
    font-weight: 600;
  }
  .badge.fresh {
    background: color-mix(in srgb, var(--ax-success) 22%, transparent);
    color: var(--ax-success);
  }
  .badge.stale {
    background: color-mix(in srgb, var(--ax-warning) 22%, transparent);
    color: var(--ax-warning);
  }

  /* A count, not something at work: plain text like the other tiles' numbers (the accent rule). */
  .files strong {
    font-size: var(--ax-font-size-xl);
    color: var(--ax-text);
    font-weight: 700;
  }

  p {
    margin: 0;
  }
  .path {
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .muted {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
  .error {
    color: var(--ax-danger);
    font-size: var(--ax-font-size-sm);
  }
</style>
