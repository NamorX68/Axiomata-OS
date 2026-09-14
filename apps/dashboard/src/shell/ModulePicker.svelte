<!--
  "Add module" dialog: lists every registered module type. Picking one
  creates an instance via core/lifecycle (default size, cascaded position)
  and closes the dialog. A placed `singleton` module is shown but disabled.
-->
<script lang="ts">
  import { createInstance, isPlacedSingleton } from "../core/lifecycle";
  import { listModules } from "../core/registry";
  import { instances } from "../core/stores";
  import { toast } from "../core/toast";
  import Window from "./Window.svelte";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  const modules = listModules();
  // Re-evaluated whenever the instance list changes.
  const placed = $derived.by(() => {
    void $instances;
    return new Set(modules.filter((m) => isPlacedSingleton(m.type)).map((m) => m.type));
  });

  function pick(type: string) {
    const result = createInstance(type);
    if (result.ok) {
      open = false;
    } else {
      toast(result.reason, "warning");
    }
  }
</script>

{#if open}
  <Window
    title="Add module"
    onClose={() => (open = false)}
    style="width: min(520px, calc(100vw - 2 * var(--ax-space-5))); max-height: calc(100vh - 2 * var(--ax-space-5));"
  >
    {#if modules.length === 0}
      <p class="empty">No modules registered.</p>
    {:else}
      <ul>
        {#each modules as m (m.type)}
          <li>
            <button
              type="button"
              class="entry"
              disabled={placed.has(m.type)}
              onclick={() => pick(m.type)}
            >
              <span class="icon" aria-hidden="true">{@html m.icon}</span>
              <span class="text">
                <span class="title">{m.title}</span>
                <span class="meta">
                  {m.background ? "background" : (m.sizeLabel ?? `${m.defaultSize.w}×${m.defaultSize.h}`)}
                  {#if m.singleton}· single{placed.has(m.type) ? " · placed" : ""}{/if}
                </span>
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </Window>
{/if}

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: var(--ax-space-2);
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: var(--ax-space-2);
  }

  .entry {
    width: 100%;
    display: flex;
    align-items: center;
    gap: var(--ax-space-3);
    padding: var(--ax-space-3);
    text-align: left;
    background: var(--ax-surface-2);
  }
  .entry:hover:not(:disabled) {
    background: var(--ax-surface-3);
    border-color: var(--ax-accent);
  }

  .icon {
    display: inline-flex;
    width: 22px;
    height: 22px;
    flex: 0 0 auto;
    color: var(--ax-accent);
  }
  .icon :global(svg) {
    width: 100%;
    height: 100%;
  }

  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .title {
    font-weight: 600;
  }
  .meta {
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }

  .empty {
    margin: 0;
    padding: var(--ax-space-4);
    color: var(--ax-text-muted);
  }
</style>
