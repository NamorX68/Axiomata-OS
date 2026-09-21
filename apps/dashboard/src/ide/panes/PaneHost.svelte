<!--
  One pane's content: the registered module named by the tab's `kind`, mounted
  with a pane-flavoured `ModuleContext` (`ide/moduleAdapter.ts`).

  Two things this component exists to get right:

  * **It mounts once per tab and stays mounted**, including while its tab is
    hidden behind another in the same group. `PaneGroup` hides the inactive
    ones with `visibility`, never `{#if}`. A terminal closes its PTY session in
    `onDestroy`, so unmounting on tab switch would kill the shell — and with it
    whatever was running in it — every time the user looked at a sibling tab.
    `visibility: hidden` rather than `display: none` for the same family of
    reason: a hidden pane keeps its measured size, and the terminal resizes its
    PTY from a `ResizeObserver` that would otherwise report 0×0 and tell the
    shell it had zero rows.
  * **An unknown kind draws a placeholder, not nothing.** A layout written by a
    later version can name a pane this build has never heard of; saying so is
    honest, and it keeps the tab there so the layout survives being opened by
    the older build (`layout.ts`'s reasoning for `kind` being a plain string).
-->
<script lang="ts">
  import { getModule } from "../../core/registry";
  import type { PaneTab } from "../layout";
  import { paneContext } from "../moduleAdapter";
  import { session } from "../projectSession";
  import AgentPane from "./AgentPane.svelte";

  let { tab, onConfig }: { tab: PaneTab; onConfig: (config: Record<string, unknown>) => void } = $props();

  // A tab's id and kind never change while it is mounted — the group keys on
  // the id — so capturing both, and the context built from them, is intended.
  // svelte-ignore state_referenced_locally
  const def = getModule(tab.kind);
  // svelte-ignore state_referenced_locally
  const ctx = paneContext(tab, onConfig);

  /** An agent pane names its profile by id; the session holds the row. */
  const agentId = $derived(typeof tab.config?.agentId === "number" ? tab.config.agentId : null);
  const agent = $derived(agentId === null ? null : ($session.agents.find((a) => a.id === agentId) ?? null));
</script>

<!-- `data-ide-pane` is a signal, not styling: a module that behaves differently
     outside a canvas tile asks for it rather than inferring it from a missing
     ancestor (`modules/terminal.svelte`'s `watchFlipBack`). -->
<div class="pane-host" data-ide-pane>
  {#if tab.kind === "agent"}
    {#if agent && $session.current}
      <AgentPane {agent} cwd={$session.current.repo_root} tabId={tab.id} />
    {:else}
      <!-- The profile was deleted, or belongs to a project that is not open.
           The pane stays rather than closing itself: something may still be
           running in it, and closing would take that with it. -->
      <p class="unknown">This agent profile is no longer in the open project.</p>
    {/if}
  {:else if def}
    <def.component {ctx} />
  {:else}
    <p class="unknown">No module of kind “{tab.kind}” in this build.</p>
  {/if}
</div>

<style>
  .pane-host {
    position: absolute;
    inset: 0;
    overflow: hidden;
  }

  .unknown {
    margin: 0;
    padding: var(--ax-space-4);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
</style>
