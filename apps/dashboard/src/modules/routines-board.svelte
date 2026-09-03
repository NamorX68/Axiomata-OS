<!--
  routines-board — the M3 scheduler as a TIME | ROUTINE | STATUS board (per
  the reference screenshots). The soonest enabled routine is "next", the
  rest "queued", disabled ones "off". Toggle per row. Polls every POLL_MS.
  Config (flip side): `showDisabled`, `sort` ("next" | "name"); the flip side
  also hosts the add-routine form.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import type { Routine } from "../core/backend";
  import { relativeTime, untilTime } from "../core/format";
  import type { ModuleContext } from "../core/types";

  const POLL_MS = 5000;

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  let routines = $state<Routine[]>([]);
  let error = $state("");
  let toggling = $state<number[]>([]);

  const showDisabled = $derived($config.showDisabled !== false);
  const sort = $derived($config.sort === "name" ? "name" : "next");

  const nextId = $derived.by(() => {
    const due = routines.filter((r) => r.enabled && r.next_fire_at);
    if (due.length === 0) return null;
    return due.reduce((a, b) => (Date.parse(a.next_fire_at!) <= Date.parse(b.next_fire_at!) ? a : b)).id;
  });

  const visible = $derived.by(() => {
    const list = routines.filter((r) => showDisabled || r.enabled);
    if (sort === "name") return [...list].sort((a, b) => a.name.localeCompare(b.name));
    const t = (r: Routine) => (r.enabled && r.next_fire_at ? Date.parse(r.next_fire_at) : Infinity);
    return [...list].sort((a, b) => t(a) - t(b) || a.name.localeCompare(b.name));
  });

  function statusOf(r: Routine): "next" | "queued" | "off" {
    if (!r.enabled) return "off";
    return r.id === nextId ? "next" : "queued";
  }

  async function refresh() {
    try {
      routines = await ctx.invoke<Routine[]>("list_routines");
      error = "";
    } catch (err) {
      error = String(err);
    }
  }

  async function toggle(r: Routine) {
    if (toggling.includes(r.id)) return;
    toggling = [...toggling, r.id];
    try {
      await ctx.invoke<boolean>("set_routine_enabled", { id: r.id, enabled: !r.enabled });
    } catch (err) {
      error = `toggle #${r.id}: ${String(err)}`;
    } finally {
      toggling = toggling.filter((id) => id !== r.id);
      await refresh();
    }
  }

  onMount(() => {
    void refresh();
    const id = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(id);
  });
</script>

<div class="board">
  {#if error}
    <p class="error">{error}</p>
  {/if}
  {#if visible.length === 0 && !error}
    <p class="muted">{routines.length === 0 ? "No routines yet — add one on the flip side." : "All routines are disabled."}</p>
  {:else}
    <table>
      <thead>
        <tr><th>Time</th><th>Routine</th><th>Status</th><th></th></tr>
      </thead>
      <tbody>
        {#each visible as r (r.id)}
          {@const st = statusOf(r)}
          <tr class={st} title="{r.cron_expr} · {r.target.type}: {r.target.value} · last {relativeTime(r.last_fired_at)}">
            <td class="time">{r.enabled ? untilTime(r.next_fire_at) : "—"}</td>
            <td class="name">
              {r.name}
              <span class="target">{r.target.type === "skill" ? `/${r.target.value}` : "prompt"}</span>
            </td>
            <td class="status"><span class="pill {st}">{st}</span></td>
            <td class="act">
              <button
                type="button"
                disabled={toggling.includes(r.id)}
                aria-label={r.enabled ? `Disable ${r.name}` : `Enable ${r.name}`}
                title={r.enabled ? "Disable" : "Enable"}
                onclick={() => toggle(r)}
              >
                {r.enabled ? "on" : "off"}
              </button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .board {
    padding: var(--ax-space-2) var(--ax-space-3) var(--ax-space-3);
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }
  th {
    text-align: left;
    padding: var(--ax-space-1) var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    font-weight: 600;
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    color: var(--ax-text-muted);
    border-bottom: 1px solid var(--ax-border);
  }
  td {
    padding: var(--ax-space-1) var(--ax-space-2);
    border-bottom: 1px solid var(--ax-border);
    vertical-align: middle;
    white-space: nowrap;
  }
  tr:last-child td {
    border-bottom: none;
  }
  tr.off td {
    color: var(--ax-text-muted);
  }

  .time {
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-accent);
  }
  tr.off .time {
    color: var(--ax-text-muted);
  }
  .name {
    width: 100%;
    white-space: normal;
    font-weight: 600;
  }
  .target {
    margin-left: var(--ax-space-2);
    font-weight: 400;
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }

  .pill {
    display: inline-block;
    padding: 1px var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    font-size: var(--ax-font-size-sm);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ax-text-muted);
    background: var(--ax-surface-3);
  }
  .pill.next {
    color: var(--ax-text-invert);
    background: var(--ax-accent);
  }
  .pill.queued {
    color: var(--ax-text);
  }

  .act button {
    min-width: 40px;
    padding: 1px var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }

  p {
    margin: 0;
  }
  .muted {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
  .error {
    color: var(--ax-danger);
    font-size: var(--ax-font-size-sm);
    margin-bottom: var(--ax-space-2);
  }
</style>
