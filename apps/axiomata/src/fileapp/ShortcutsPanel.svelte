<!--
  The Shortcuts tab of the file app's inspector (`docs/plans/editor-look.md`,
  LK1, K2): every key from `shortcuts.ts`, grouped, searchable by key or word;
  the Vi groups only while Vi is on.
  Each shortcut is a row — what it does on the left, its keys on the right — and the keys wrap inside their own
  column: a long chord (Windows/Linux spell `Ctrl+Shift+…` out) never runs into the text beside it (card #102).
-->
<script lang="ts">
  import Icon from "../ui/Icon.svelte";
  import { platform } from "../core/keys";
  import { editorSettings } from "./editorSettings";
  import { visibleShortcuts } from "./shortcuts";

  let query = $state("");
  const groups = $derived(visibleShortcuts($editorSettings.mode === "vi", query, $platform));
  const total = $derived(groups.reduce((sum, group) => sum + group.items.length, 0));
</script>

<div class="shortcuts">
  <div class="head">
    <label class="search">
      <Icon name="search" size="sm" />
      <input type="search" placeholder="Search keys or actions" spellcheck="false" bind:value={query} />
    </label>
    <span class="count" aria-live="polite">{total} {total === 1 ? "shortcut" : "shortcuts"}</span>
  </div>
  <div class="groups">
    {#each groups as group (group.title)}
      <section>
        <h3>
          <span>{group.title}</span>
          <span class="n">{group.items.length}</span>
        </h3>
        <ul>
          {#each group.items as item (item.keys + item.what)}
            <li>
              <span class="what">{item.what}</span>
              <span class="keys">
                {#each item.keys.split(" / ") as alternative, i (i)}
                  {#if i > 0}<span class="or">/</span>{/if}<kbd>{alternative}</kbd>
                {/each}
              </span>
            </li>
          {/each}
        </ul>
      </section>
    {:else}
      <div class="none">
        <Icon name="search" />
        <p>No shortcut matches “{query}”.</p>
      </div>
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

  .head {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    padding: var(--ax-space-3) var(--ax-space-4) var(--ax-space-2);
    border-bottom: 1px solid var(--ax-border);
  }

  .search {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-2) var(--ax-space-3);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    background: var(--ax-surface-1);
    color: var(--ax-text-muted);
  }

  .search:focus-within {
    border-color: var(--ax-border-strong);
    background: var(--ax-elev-2);
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

  .count {
    align-self: flex-end;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .groups {
    flex: 1;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-4);
    padding: var(--ax-space-3) var(--ax-space-4) var(--ax-space-5);
  }

  /* A group is a plate of its own; its heading stays in view while its rows scroll past. */
  section {
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    background: var(--ax-elev-2);
    box-shadow: var(--ax-shadow-raised);
  }

  h3 {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 0;
    padding: var(--ax-space-2) var(--ax-space-3);
    border-bottom: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md) var(--ax-radius-md) 0 0;
    background: var(--ax-elev-3);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    font-weight: 600;
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
  }

  .n {
    font-weight: 400;
    letter-spacing: 0;
  }

  ul {
    margin: 0;
    padding: var(--ax-space-1) 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ax-space-3);
    padding: var(--ax-space-1) var(--ax-space-3);
    font-size: var(--ax-font-size-sm);
  }

  li + li {
    border-top: 1px solid color-mix(in srgb, var(--ax-border) 50%, transparent);
  }

  li:hover {
    background: var(--ax-elev-3);
  }

  .what {
    flex: 1 1 auto;
    min-width: 0;
    color: var(--ax-text);
    line-height: 1.4;
  }

  /* The keys keep to their column and wrap inside it, right-aligned: nothing reaches into the text. */
  .keys {
    flex: 0 0 auto;
    max-width: 55%;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: var(--ax-space-1);
  }

  kbd {
    min-width: calc(20px * var(--ax-ui-scale));
    padding: calc(1px * var(--ax-ui-scale)) var(--ax-space-1);
    border: 1px solid var(--ax-border-strong);
    border-bottom-width: calc(2px * var(--ax-ui-scale));
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-1);
    color: var(--ax-text);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    text-align: center;
    white-space: nowrap;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .or {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .none {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-6) 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }

  .none p {
    margin: 0;
  }
</style>
