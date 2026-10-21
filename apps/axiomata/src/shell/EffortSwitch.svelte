<!--
  The chat's effort switch (`docs/plans/glut-effort.md`, F2): how hard Glut's model reasons in this conversation,
  with the levels the chat model takes. Picking one is `/effort <level>` — this conversation only; "default" goes back
  to the configured chat effort. Shown only while the chat runs on Glut: the other harnesses keep their own settings
  (E6). The config is read when the switch mounts (each time the chat panel opens) and after every Settings save.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { CONFIG_SAVED, type ConfigView, invokeBackend, isEffortLevel } from "../core/backend";
  import { on } from "../core/bus";
  import { busy, turnEffort, turnModel } from "../core/chat";
  import {
    defaultLabel,
    effortState,
    offeredLevels,
    reasoningOf,
    takesLevel,
    unlistedNote,
  } from "../core/effortLevels";
  import { loadModelCatalog, modelCatalog } from "../core/modelCatalog";

  /** The `<select>` value of "no level for this conversation". */
  const DEFAULT = "";

  let config = $state<ConfigView | null>(null);

  function readConfig(): void {
    invokeBackend<ConfigView>("get_config")
      .then((c) => (config = c))
      .catch(() => {
        // Kept as it was (hidden before the first read); `/effort` still answers.
      });
  }

  onMount(() => {
    readConfig();
    if ($modelCatalog === null) {
      loadModelCatalog().catch(() => {
        // Without a catalog every level is offered.
      });
    }
    return on(CONFIG_SAVED, readConfig);
  });

  const onGlut = $derived(config?.agents.chat_harness === "axh");
  const provider = $derived(config?.agents.chat_provider ?? "open_router");
  const model = $derived($turnModel ?? config?.agents.providers[provider]?.chat_model ?? "");
  const reasoning = $derived(reasoningOf($modelCatalog, provider, model));
  const configured = $derived(config?.agents.chat_effort ?? null);
  const current = $derived(effortState($turnEffort, configured, reasoning));
  const levels = $derived(offeredLevels(reasoning, $turnEffort));
  const noDefault = $derived(configured ? `default (${configured})` : defaultLabel(reasoning));
  const note = $derived(unlistedNote(reasoning, current.level));
  const title = $derived(
    `Effort: ${current.explanation}${note ? ` (${note})` : ""} — this conversation only; /effort lists the levels`,
  );

  function onchange(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value;
    turnEffort.set(isEffortLevel(value) ? value : null);
  }
</script>

{#if onGlut}
  <label class="effort" {title}>
    <span>Effort</span>
    <select value={$turnEffort ?? DEFAULT} {onchange} disabled={$busy} aria-label="Effort for this conversation">
      <option value={DEFAULT}>{noDefault}</option>
      {#each levels as level (level)}
        <option value={level}>{level}{takesLevel(reasoning, level) ? "" : " (not listed)"}</option>
      {/each}
    </select>
  </label>
{/if}

<style>
  .effort {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }

  .effort select {
    font-size: var(--ax-font-size-sm);
    padding: calc(1px * var(--ax-ui-scale)) var(--ax-space-1);
  }
</style>
