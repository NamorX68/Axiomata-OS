<!--
  The Shortcuts tab of the file app's inspector (`docs/plans/editor-look.md`,
  LK1, K2): every key from `shortcuts.ts`, grouped, searchable by key or word;
  the Vi groups only while Vi is on.
-->
<script lang="ts">
  import Icon from "../ui/Icon.svelte";
  import { editorSettings } from "./editorSettings";
  import { visibleShortcuts } from "./shortcuts";

  let query = $state("");
  const groups = $derived(visibleShortcuts($editorSettings.mode === "vi", query));
</script>

<div class="shortcuts">
  <label class="search">
    <Icon name="search" size="sm" />
    <input type="search" placeholder="Search keys or actions" spellcheck="false" bind:value={query} />
  </label>
  <div class="groups">
    {#each groups as group (group.title)}
      <section>
        <h3>{group.title}</h3>
        <dl>
          {#each group.items as item (item.keys)}
            <dt>
              {#each item.keys.split(" / ") as alternative, i (i)}
                {#if i > 0}<span class="or">/</span>{/if}<kbd>{alternative}</kbd>
              {/each}
            </dt>
            <dd>{item.what}</dd>
          {/each}
        </dl>
      </section>
    {:else}
      <p class="none">No shortcut matches “{query}”.</p>
    {/each}
  </div>
</div>

<style>
  .shortcuts {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .search {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    margin: var(--ax-space-3) var(--ax-space-4);
    padding: var(--ax-space-1) var(--ax-space-3);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    background: var(--ax-surface-1);
    color: var(--ax-text-muted);
  }

  .search:focus-within {
    border-color: var(--ax-accent);
  }

  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--ax-text);
    font: inherit;
    font-size: var(--ax-font-size-sm);
  }

  .groups {
    flex: 1;
    overflow: auto;
    padding: 0 var(--ax-space-4) var(--ax-space-4);
  }

  section + section {
    margin-top: var(--ax-space-4);
  }

  h3 {
    margin: 0 0 var(--ax-space-2);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    font-weight: 600;
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
  }

  dl {
    display: grid;
    grid-template-columns: minmax(auto, 45%) 1fr;
    gap: var(--ax-space-1) var(--ax-space-3);
    margin: 0;
    font-size: var(--ax-font-size-sm);
  }

  dt {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--ax-space-1);
  }

  dd {
    margin: 0;
    color: var(--ax-text);
  }

  kbd {
    padding: 0 var(--ax-space-1);
    border: 1px solid var(--ax-border);
    border-bottom-width: 2px;
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-1);
    color: var(--ax-text);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    white-space: nowrap;
  }

  .or {
    color: var(--ax-text-muted);
  }

  .none {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
</style>
