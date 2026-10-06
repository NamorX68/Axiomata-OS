<!--
  The choice at the start of a planner or a grilling session (the plan's panel): a **role**, not an engine. The role carries
  its engine, shown beside its name; only a role that names none (a freshly seeded one) asks for an engine — once: the
  start saves the pick into the role, and the next start is one click. Engines and roles are changed under "Engines &
  roles".
-->
<script lang="ts">
  import type { EngineEntry, Role } from "../../core/roster";
  import { engineLine } from "../rosterStore";
  import { sessionEngine } from "../planning";

  let {
    label,
    names,
    roles,
    catalog,
    role = $bindable(""),
    engine = $bindable(""),
  }: {
    label: string;
    /** The role names to offer, the seeded one first. */
    names: string[];
    roles: Role[];
    catalog: EngineEntry[];
    /** The role picked; empty = the first. */
    role: string;
    /** The engine picked for a role that has none. */
    engine: string;
  } = $props();

  const picked = $derived(names.includes(role) ? role : (names[0] ?? ""));
  const current = $derived(roles.find((r) => r.name === picked) ?? null);
  const choice = $derived(sessionEngine(current, catalog, engine));
  const engineLabel = $derived(catalog.find((e) => e.id === choice.engineId));
</script>

<label class="picker">
  {label}
  <select value={picked} aria-label={label} onchange={(event) => (role = (event.currentTarget as HTMLSelectElement).value)}>
    {#each names as name (name)}<option value={name}>{name}</option>{/each}
  </select>
</label>
{#if choice.ask}
  <label class="picker">
    Engine für „{picked}“ <span class="muted">(wird in der Rolle gespeichert)</span>
    <select
      value={choice.engineId}
      aria-label="Engine für {picked}"
      onchange={(event) => (engine = (event.currentTarget as HTMLSelectElement).value)}
    >
      {#each catalog as e (e.id)}<option value={e.id}>{e.label} — {engineLine(e)}</option>{/each}
    </select>
  </label>
{:else if engineLabel}
  <span class="muted on" title="Die Engine gehört zur Rolle; ändern unter Engines &amp; roles">läuft auf {engineLabel.label}</span>
{/if}

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  .picker select {
    background: var(--ax-surface-1);
    color: var(--ax-text);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    padding: var(--ax-space-1) var(--ax-space-2);
    font: inherit;
    font-size: var(--ax-font-size-sm);
  }
  .muted {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  .on {
    align-self: flex-end;
  }
</style>
