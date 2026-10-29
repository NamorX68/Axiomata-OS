<!--
  skills-deck — the skills registry as a card grid (per the reference
  screenshots): icon, `/name`, backend, a ▶ run button and the last run's
  outcome. Polls skills + recent runs every POLL_MS. Config (flip side):
  `filterBackend` ("" = all) and `order` ("name" | "recent").

  The bundled skills are read once at mount: when one is outdated or missing, a quiet neutral notice links to the
  flip side's "Bundled skills" section (an update waits for no one, so no glow).

  Calm at rest ("Glut + Hex", UX3): the skill that runs is the one card that glows — its icon lights up and its run
  button turns into a quiet "busy" mark; the last outcome is a hex mark.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { type BundledSkill, type RunSummary, type Skill, type SkippedSkill, runFailed } from "../core/backend";
  import { relativeTime } from "../core/format";
  import { backendLabel } from "../core/harness";
  import { updateInstance } from "../core/stores";
  import type { ModuleContext } from "../core/types";
  import { ring } from "../ui/glow";
  import Notice from "../ui/Notice.svelte";
  import StatusMark from "../ui/StatusMark.svelte";
  import { bundledFocus, bundledSkills, noticeText } from "./bundledSkills";

  const POLL_MS = 5000;
  const RUN_LIMIT = 50;

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  let skills = $state<Skill[]>([]);
  let runs = $state<RunSummary[]>([]);
  let skipped = $state<SkippedSkill[]>([]);
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
    const list = skills.filter((s) => filterBackend === "" || s.harness === filterBackend);
    if (order === "recent") {
      const t = (s: Skill) => Date.parse(lastRun.get(s.name)?.started_at ?? "") || 0;
      return [...list].sort((a, b) => t(b) - t(a) || a.name.localeCompare(b.name));
    }
    return [...list].sort((a, b) => a.name.localeCompare(b.name));
  });

  async function refresh() {
    try {
      [skills, runs, skipped] = await Promise.all([
        ctx.invoke<Skill[]>("list_skills"),
        ctx.invoke<RunSummary[]>("list_runs", { limit: RUN_LIMIT }),
        ctx.invoke<SkippedSkill[]>("list_skipped_skills"),
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

  /** Goes on with an interrupted Glut run from its journal — only on the owner's click (durable runs, D3). */
  async function resume(name: string, id: number) {
    if (running.includes(name)) return;
    running = [...running, name];
    try {
      await ctx.invoke<RunSummary>("resume_run", { id });
    } catch (err) {
      error = `resume ${name}: ${String(err)}`;
    } finally {
      running = running.filter((n) => n !== name);
      await refresh();
    }
  }

  /** Gives an interrupted run up: its journal goes, the run stays interrupted. */
  async function discard(name: string, id: number) {
    try {
      await ctx.invoke("discard_run", { id });
    } catch (err) {
      error = `drop ${name}: ${String(err)}`;
    }
    await refresh();
  }

  const bundledNotice = $derived(noticeText($bundledSkills));

  /** Turns the tile to its settings and asks them to show the "Bundled skills" section. */
  function showBundled() {
    bundledFocus.update((n) => n + 1);
    updateInstance(ctx.instanceId, { flipped: true });
  }

  onMount(() => {
    // Once per start, not with every poll: the bundled versions only change with a new build of the app.
    ctx
      .invoke<BundledSkill[]>("bundled_skills_status")
      .then((list) => bundledSkills.set(list))
      .catch(() => {
        // The notice is a hint only; the settings' section reports a failure to read.
      });
    void refresh();
    const id = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(id);
  });
</script>

<div class="deck">
  {#if error}
    <p class="error">{error}</p>
  {/if}
  {#if bundledNotice}
    <div class="bundled">
      <Notice title={bundledNotice} tone="neutral">
        {#snippet action()}
          <button type="button" class="ax-btn" onclick={showBundled}>Review</button>
        {/snippet}
      </Notice>
    </div>
  {/if}
  {#if skipped.length > 0}
    <ul class="skipped" title="A skill directory with a broken or missing SKILL.md — see ~/.axiomata/skills/">
      {#each skipped as s (s.name)}
        <li>⚠ skipped <strong>{s.name}</strong>: {s.reason}</li>
      {/each}
    </ul>
  {/if}
  {#if visible.length === 0 && !error}
    <p class="muted">{skills.length === 0 ? "No skills in ~/.axiomata/skills/." : "No skills match the filter."}</p>
  {:else}
    <ul class="grid">
      {#each visible as s (s.name)}
        {@const last = lastRun.get(s.name)}
        {@const busy = running.includes(s.name)}
        <li class="card" class:live={busy} title={s.description} use:ring={busy ? "working" : null}>
          <span class="icon" aria-hidden="true">
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round">
              <path d="M9 1.5 3.5 9H8l-1 5.5L12.5 7H8z" />
            </svg>
          </span>
          <span class="name">/{s.name}</span>
          <span class="meta">
            <span class="backend" title={s.backend ? "set in the skill" : "the skill harness of the settings"}>
              {backendLabel(s.harness)}
            </span>
            {#if busy}
              <span class="running">running</span>
            {:else if last}
              <StatusMark
                tone={last.status === "running" ? "live" : runFailed(last.status) ? "bad" : "ok"}
                label={last.status}
                title="{last.status} · {relativeTime(last.started_at)}"
              />
              {#if last.resumable}
                <button
                  type="button"
                  class="link"
                  title="Go on from where the run stopped"
                  onclick={() => resume(s.name, last.id)}>resume</button
                >
                <button
                  type="button"
                  class="link"
                  title="Give the interrupted run up"
                  onclick={() => discard(s.name, last.id)}>drop</button
                >
              {/if}
            {/if}
          </span>
          <button
            type="button"
            class="run"
            disabled={busy}
            aria-label="Run {s.name}"
            aria-busy={busy}
            title={busy ? "Running…" : "Run"}
            onclick={() => run(s.name)}
          >
            {#if busy}
              <!-- The card already glows: one glowing cue is enough, the mark only says "busy". -->
              <StatusMark tone="neutral" size="md" />
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
    /* Wide enough for a skill's name: at 150 px a wider tile fitted three and cut every name (owner, 2026-10-11). */
    grid-template-columns: repeat(auto-fill, minmax(calc(240px * var(--ax-ui-scale)), 1fr));
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
    width: calc(18px * var(--ax-ui-scale));
    height: calc(18px * var(--ax-ui-scale));
    color: var(--ax-text-muted);
  }
  .card.live .icon,
  .running {
    color: var(--ax-glow);
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
    min-width: 0;
    font: var(--ax-type-caption);
    color: var(--ax-text-muted);
    white-space: nowrap;
  }

  .run {
    grid-column: 3;
    grid-row: 1 / span 2;
    width: calc(28px * var(--ax-ui-scale));
    height: calc(28px * var(--ax-ui-scale));
    padding: 0;
    display: grid;
    place-items: center;
    border-radius: var(--ax-radius-sm);
    border-color: var(--ax-border-strong);
    color: var(--ax-text-muted);
    background: transparent;
  }
  /* Resume / drop of an interrupted run: words in the meta line, not a second button beside Run. */
  .link {
    padding: 0;
    border: none;
    background: none;
    color: var(--ax-text-muted);
    font-size: inherit;
    text-decoration: underline;
    cursor: pointer;
  }
  .link:hover {
    color: var(--ax-text);
  }
  .run:hover:not(:disabled) {
    background: var(--ax-selected-bg);
    color: var(--ax-text);
  }
  .run:disabled {
    opacity: 1;
    border-color: color-mix(in srgb, var(--ax-glow) 50%, transparent);
  }
  .run svg {
    width: calc(14px * var(--ax-ui-scale));
    height: calc(14px * var(--ax-ui-scale));
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
  .bundled {
    margin-bottom: var(--ax-space-2);
  }
  .skipped {
    list-style: none;
    margin: 0 0 var(--ax-space-2);
    padding: 0;
    color: var(--ax-warning);
    font-size: var(--ax-font-size-sm);
  }
</style>
