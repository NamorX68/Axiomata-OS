<!--
  The outline under the file tree (`docs/plans/editor-projekt-werkzeuge.md`, #49): the front
  file's symbols as a tree. A click jumps to the symbol; the row the cursor is in is marked and
  kept in view; a filter narrows the tree by name; the pin stops the mark from following the
  cursor, so the outline can be used as a map of where you were.
-->
<script lang="ts">
  import { tick } from "svelte";

  import { filterOutline, pathAt, type OutlineSymbol, type SymbolKind } from "../editor/syntax/outline";
  import Icon from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import type { OutlineInfo } from "./outlineModel";

  interface Props {
    info: OutlineInfo | null;
    open: boolean;
    onToggle: () => void;
    onJump: (line: number) => void;
  }

  let { info, open, onToggle, onJump }: Props = $props();

  let query = $state("");
  let pinned = $state(false);
  /** Symbols folded away, by `keyOf`. */
  let folded = $state<Set<string>>(new Set());
  /** The cursor line the mark follows — frozen while pinned. */
  let markLine = $state(0);
  let list = $state<HTMLElement | null>(null);

  const keyOf = (s: OutlineSymbol) => `${s.line}\0${s.name}`;

  $effect(() => {
    if (!pinned && info) markLine = info.line;
  });

  const shown = $derived(info?.symbols ? filterOutline(info.symbols, query) : []);
  const marked = $derived(new Set((info?.symbols ? pathAt(info.symbols, markLine) : []).map(keyOf)));
  /** The innermost symbol holding the cursor: the one row that gets the strong mark. */
  const current = $derived.by(() => {
    const path = info?.symbols ? pathAt(info.symbols, markLine) : [];
    return path.length > 0 ? keyOf(path[path.length - 1]) : null;
  });

  // Keep the current row in view as the cursor moves (not while the filter is being typed).
  $effect(() => {
    void current;
    if (!open || query !== "") return;
    void tick().then(() => list?.querySelector<HTMLElement>('[aria-current="true"]')?.scrollIntoView({ block: "nearest" }));
  });

  const GLYPH: Record<SymbolKind, string> = {
    function: "fn",
    method: "fn",
    class: "C",
    struct: "S",
    enum: "E",
    interface: "I",
    trait: "T",
    impl: "im",
    module: "M",
    constant: "K",
    type: "t",
    macro: "!",
    heading: "#",
  };

  function toggleFold(s: OutlineSymbol): void {
    const next = new Set(folded);
    if (!next.delete(keyOf(s))) next.add(keyOf(s));
    folded = next;
  }
</script>

{#snippet row(s: OutlineSymbol, depth: number)}
  {@const key = keyOf(s)}
  {@const isFolded = query === "" && folded.has(key)}
  <div
    class="row"
    class:marked={marked.has(key)}
    class:current={current === key}
    style:--depth={depth}
    aria-current={current === key ? "true" : undefined}
  >
    {#if s.children.length > 0}
      <button
        type="button"
        class="chevron"
        aria-label={isFolded ? "Unfold" : "Fold"}
        onclick={() => toggleFold(s)}
        ><Icon name={isFolded ? "chevron-right" : "chevron-down"} size="sm" /></button
      >
    {:else}
      <span class="chevron"></span>
    {/if}
    <button type="button" class="jump" onclick={() => onJump(s.nameLine)} title="Line {s.line + 1}">
      <span class="glyph kind-{s.kind}" aria-hidden="true">{GLYPH[s.kind]}</span>
      <span class="name">{s.name}</span>
    </button>
  </div>
  {#if !isFolded}
    {#each s.children as child (keyOf(child))}
      {@render row(child, depth + 1)}
    {/each}
  {/if}
{/snippet}

<section class="outline" aria-label="Outline">
  <header>
    <button type="button" class="title" aria-expanded={open} onclick={onToggle}>
      <Icon name={open ? "chevron-down" : "chevron-right"} size="sm" /> Outline
    </button>
    {#if open}
      <IconButton
        icon="lock"
        size="sm"
        pressed={pinned}
        label={pinned ? "Follow the cursor again" : "Pin: stop following the cursor"}
        onclick={() => {
          pinned = !pinned;
          if (!pinned && info) markLine = info.line;
        }}
      />
    {/if}
  </header>
  {#if open}
    <input class="filter" type="text" spellcheck="false" placeholder="Filter symbols" bind:value={query} />
    <div class="list" bind:this={list}>
      {#if !info}
        <p class="note">No file open.</p>
      {:else if info.symbols === null}
        <p class="note">No outline for this file.</p>
      {:else if shown.length === 0}
        <p class="note">{query === "" ? "No symbols." : "Nothing matches."}</p>
      {:else}
        {#each shown as s (keyOf(s))}
          {@render row(s, 0)}
        {/each}
      {/if}
    </div>
  {/if}
</section>

<style>
  .outline {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    border-top: 1px solid var(--ax-border);
    background: var(--ax-surface-1);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 var(--ax-space-2);
  }

  .title {
    display: flex;
    align-items: center;
    gap: var(--ax-space-1);
    padding: var(--ax-space-1) var(--ax-space-1);
    background: none;
    border: 0;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    cursor: pointer;
  }

  .filter {
    margin: 0 var(--ax-space-3) var(--ax-space-1);
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
  }

  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding-bottom: var(--ax-space-2);
  }

  .note {
    margin: 0;
    padding: var(--ax-space-2) var(--ax-space-4);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }

  .row {
    display: flex;
    align-items: center;
    padding-left: calc(var(--ax-space-2) + var(--depth) * var(--ax-space-3));
    color: var(--ax-text);
    font-size: var(--ax-font-size-sm);
  }

  .row:hover {
    background: var(--ax-accent-muted);
  }

  .row.marked .name {
    color: var(--ax-accent);
  }

  .row.current {
    background: var(--ax-accent-muted);
  }

  .chevron {
    display: inline-flex;
    flex-shrink: 0;
    width: calc(16px * var(--ax-ui-scale));
    background: none;
    border: 0;
    color: var(--ax-text-muted);
    cursor: pointer;
  }

  .jump {
    display: flex;
    flex: 1;
    align-items: center;
    gap: var(--ax-space-2);
    min-width: 0;
    padding: var(--ax-space-1) var(--ax-space-2) var(--ax-space-1) 0;
    background: none;
    border: 0;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .glyph {
    flex-shrink: 0;
    min-width: calc(18px * var(--ax-ui-scale));
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    text-align: center;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
