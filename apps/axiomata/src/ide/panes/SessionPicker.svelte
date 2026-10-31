<!--
  The choice at the start of a planner or a grilling session (the plan's panel): a **role**, not an engine. One role is
  suggested (`suggestedRole`), the owner may pick another. The role carries its engine, shown beside its name; only a
  role that names none (a freshly seeded one) asks for an engine — once: the start saves the pick into the role, and the
  next start is one click. Engines and roles are changed under "Models & roles".
-->
<script lang="ts">
  import type { EngineEntry, Role } from "../../core/roster";
  import { engineLine } from "../rosterStore";
  import { sessionEngine } from "../planning";
  import { tAgents as t } from "../../i18n/agents";

  let {
    label,
    names,
    roles,
    catalog,
    suggested = "",
    role = $bindable(""),
    engine = $bindable(""),
  }: {
    label: string;
    /** The role names to offer, the seeded one first. */
    names: string[];
    roles: Role[];
    catalog: EngineEntry[];
    /** The role shown while the owner has picked none (`suggestedRole`); empty = the first of `names`. */
    suggested?: string;
    /** The role picked; empty = the suggested one. */
    role: string;
    /** The engine picked for a role that has none. */
    engine: string;
  } = $props();

  const picked = $derived(names.includes(role) ? role : names.includes(suggested) ? suggested : (names[0] ?? ""));
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
    {t("engineFor", { role: picked })} <span class="muted">{t("savedInRole")}</span>
    <select
      value={choice.engineId}
      aria-label={t("engineForLabel", { role: picked })}
      onchange={(event) => (engine = (event.currentTarget as HTMLSelectElement).value)}
    >
      {#each catalog as e (e.id)}<option value={e.id}>{e.label} — {engineLine(e)}</option>{/each}
    </select>
  </label>
{:else if engineLabel}
  <span class="muted on" title={t("engineBelongsToRole")}>{t("runsOn", { engine: engineLabel.label })}</span>
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
