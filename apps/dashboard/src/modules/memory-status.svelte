<!--
  memory-status — the M2 router at a glance: workspace, tracked files, last
  sync, fresh/stale badge, and a Sync button. Polls every POLL_MS while
  mounted. `config.compact` (flip side) collapses it to a single badge line.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import type { MemoryStatus, SyncReport } from "../core/backend";
  import { relativeTime, shortPath } from "../core/format";
  import type { ModuleContext } from "../core/types";

  const POLL_MS = 3000;

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  let status = $state<MemoryStatus | null>(null);
  let error = $state("");
  let syncing = $state(false);
  let lastReport = $state("");

  const compact = $derived($config.compact === true);

  async function refresh() {
    try {
      status = await ctx.invoke<MemoryStatus>("get_memory_status");
      error = "";
    } catch (err) {
      error = String(err);
    }
  }

  async function sync() {
    if (syncing) return;
    syncing = true;
    try {
      const r = await ctx.invoke<SyncReport>("sync_memory");
      const base =
        r.written.length === 0
          ? `In sync · ${r.tracked_files} files`
          : `Wrote ${r.written.length} · ${r.unchanged} unchanged`;
      lastReport = r.failed.length === 0 ? base : `${base} · ${r.failed.length} failed`;
    } catch (err) {
      lastReport = `Sync failed: ${String(err)}`;
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
    <p class="muted">Loading…</p>
  {:else}
    <div class="row">
      <span class="badge" class:stale={status.stale} class:fresh={!status.stale}>
        {status.stale ? "stale" : "fresh"}
      </span>
      <span class="files"><strong>{status.tracked_files}</strong> files</span>
      <span class="muted">synced {relativeTime(status.last_sync)}</span>
      <button type="button" class="sync" disabled={syncing} onclick={sync}>
        {syncing ? "Syncing…" : "Sync"}
      </button>
    </div>
    {#if !compact}
      <p class="path" title={status.workspace_root}>{shortPath(status.workspace_root, 3)}</p>
      {#if lastReport}<p class="muted report">{lastReport}</p>{/if}
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
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .badge.fresh {
    background: color-mix(in srgb, var(--ax-success) 22%, transparent);
    color: var(--ax-success);
  }
  .badge.stale {
    background: color-mix(in srgb, var(--ax-warning) 22%, transparent);
    color: var(--ax-warning);
  }

  .files strong {
    font-size: var(--ax-font-size-xl);
    color: var(--ax-accent);
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
