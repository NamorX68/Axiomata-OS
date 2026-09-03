<!--
  skills-deck — the skills registry as a card grid (per the reference
  screenshots): icon, `/name`, backend, a ▶ run button and the last run's
  outcome. Polls skills + recent runs every POLL_MS. Config (flip side):
  `filterBackend` ("" = all) and `order` ("name" | "recent").
-->
<script lang="ts">
  import { onMount } from "svelte";

  import type { RunSummary, Skill } from "../core/backend";
  import { relativeTime } from "../core/format";
  import type { ModuleContext } from "../core/types";

  const POLL_MS = 5000;
  const RUN_LIMIT = 50;

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  let skills = $state<Skill[]>([]);
  let runs = $state<RunSummary[]>([]);
  let error = $state("");
  let running = $state<string[]>([]);

  const filterBackend = $derived(typeof $config.filterBackend === "string" ? $config.filterBackend : "");
  const order = $derived($config.order === "recent" ? "recent" : "name");

  /** Latest run per skill name. `runs` is newest-first. */
  const lastRun = $derived.by(() => {
    const map = new Map<string, RunSummary>();
    for (const r of runs) if (!map.has(r.skill_name)) map.set(r.skill_name, r);
    return map;
  });

  const visible = $derived.by(() => {
    const list = skills.filter((s) => filterBackend === "" || s.backend === filterBackend);
    if (order === "recent") {
      const t = (s: Skill) => Date.parse(lastRun.get(s.name)?.started_at ?? "") || 0;
      return [...list].sort((a, b) => t(b) - t(a) || a.name.localeCompare(b.name));
    }
    return [...list].sort((a, b) => a.name.localeCompare(b.name));
  });

  async function refresh() {
    try {
      [skills, runs] = await Promise.all([
        ctx.invoke<Skill[]>("list_skills"),
        ctx.invoke<RunSummary[]>("list_runs", { limit: RUN_LIMIT }),
      ]);
      error = "";
    } catch (err) {
      error = String(err);
    }
  }

  async function run(name: string) {
    if (running.includes(name)) return;
    running = [...running, name];
    try {
      await ctx.invoke<RunSummary>("run_skill", { name });
    } catch (err) {
      error = `run ${name}: ${String(err)}`;
    } finally {
      running = running.filter((n) => n !== name);
      await refresh();
    }
  }

  onMount(() => {
    void refresh();
    const id = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(id);
  });
</script>

<div class="deck">
  {#if error}
    <p class="error">{error}</p>
  {/if}
  {#if visible.length === 0 && !error}
    <p class="muted">{skills.length === 0 ? "No skills in ~/.axiomata/skills/." : "No skills match the filter."}</p>
  {:else}
    <ul class="grid">
      {#each visible as s (s.name)}
        {@const last = lastRun.get(s.name)}
        {@const busy = running.includes(s.name)}
        <li class="card" title={s.description}>
          <span class="icon" aria-hidden="true">
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round">
              <path d="M9 1.5 3.5 9H8l-1 5.5L12.5 7H8z" />
            </svg>
          </span>
          <span class="name">/{s.name}</span>
          <span class="meta">
            <span class="backend">{s.backend}</span>
            {#if last}
              <span class="dot {last.status}" title="{last.status} · {relativeTime(last.started_at)}"></span>
            {/if}
          </span>
          <button
            type="button"
            class="run"
            disabled={busy}
            aria-label="Run {s.name}"
            title={busy ? "Running…" : "Run"}
            onclick={() => run(s.name)}
          >
            {#if busy}
              <span class="spinner" aria-hidden="true"></span>
            {:else}
              <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M5 3.5v9l7-4.5z" /></svg>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .deck {
    padding: var(--ax-space-3);
  }

  .grid {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: var(--ax-space-2);
  }

  .card {
    position: relative;
    display: grid;
    grid-template-columns: auto 1fr auto;
    grid-template-rows: auto auto;
    column-gap: var(--ax-space-2);
    align-items: center;
    padding: var(--ax-space-2) var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    min-width: 0;
  }
  .card:hover {
    border-color: var(--ax-border-strong);
  }

  .icon {
    grid-row: 1 / span 2;
    display: inline-flex;
    width: 18px;
    height: 18px;
    color: var(--ax-accent);
  }
  .icon svg {
    width: 100%;
    height: 100%;
  }

  .name {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    grid-column: 2;
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ax-text-muted);
  }
  .backend {
    color: var(--ax-accent);
    opacity: 0.85;
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--ax-text-muted);
  }
  .dot.success {
    background: var(--ax-success);
  }
  .dot.failed {
    background: var(--ax-danger);
  }

  .run {
    grid-column: 3;
    grid-row: 1 / span 2;
    width: 28px;
    height: 28px;
    padding: 0;
    display: grid;
    place-items: center;
    border-radius: var(--ax-radius-sm);
    border-color: var(--ax-accent);
    color: var(--ax-accent);
    background: var(--ax-accent-muted);
  }
  .run:hover:not(:disabled) {
    background: var(--ax-accent);
    color: var(--ax-text-invert);
  }
  .run svg {
    width: 14px;
    height: 14px;
  }

  .spinner {
    width: 12px;
    height: 12px;
    border: 2px solid var(--ax-accent-muted);
    border-top-color: var(--ax-accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
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
