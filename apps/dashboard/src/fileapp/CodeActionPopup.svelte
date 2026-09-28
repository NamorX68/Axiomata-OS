<!--
  The code-action menu (`docs/plans/editor.md`, ED6.7, L20): what the server
  offers at the cursor, numbered 1–9, its kind as a tag, the chosen one
  highlighted, and what was typed to narrow it. An action the server marked
  as not available is greyed with its reason. What it does is
  `codeActionMenu.ts`'s; this only draws it. A click takes an entry; the mouse
  never takes the focus from the text.
-->
<script lang="ts">
  import { kindTag, type CodeActionItem } from "../editor/lsp/codeActions";
  import { NUMBERED } from "./codeActionMenu";

  interface Props {
    items: CodeActionItem[];
    selected: number;
    query: string;
    /** Where the menu's top-left corner goes, in the surface's pixels. */
    x: number;
    y: number;
    onPick: (index: number) => void;
  }

  let { items, selected, query, x, y, onPick }: Props = $props();

  /** Rows visible at once; the list scrolls to keep the chosen one in view. */
  const VISIBLE_ROWS = 12;

  let list = $state<HTMLDivElement | null>(null);

  $effect(() => {
    const row = list?.children[selected] as HTMLElement | undefined;
    row?.scrollIntoView({ block: "nearest" });
  });
</script>

<div class="actions" style:left="{x}px" style:top="{y}px" role="presentation" onmousedown={(e) => e.preventDefault()}>
  <div class="query" class:empty={query === ""}>{query === "" ? "Code actions — type to filter" : query}</div>
  <div class="items" role="listbox" aria-label="Code actions" bind:this={list} style:--rows={VISIBLE_ROWS}>
    {#each items as item, i (i)}
      <button
        type="button"
        class="item"
        class:chosen={i === selected}
        class:disabled={item.disabled !== null}
        role="option"
        aria-selected={i === selected}
        aria-disabled={item.disabled !== null}
        title={item.disabled ?? item.title}
        tabindex="-1"
        onclick={() => onPick(i)}
      >
        <span class="number">{i < NUMBERED ? i + 1 : ""}</span>
        <span class="kind">{kindTag(item.kind)}</span>
        <span class="title">{item.title}</span>
        {#if item.disabled}<span class="reason">{item.disabled}</span>{/if}
      </button>
    {:else}
      <div class="none">No match</div>
    {/each}
  </div>
</div>

<style>
  .actions {
    position: absolute;
    z-index: 7;
    display: flex;
    flex-direction: column;
    min-width: 32ch;
    max-width: 80ch;
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    box-shadow: var(--ax-shadow-pop);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text);
  }

  .query {
    padding: 2px var(--ax-space-2);
    border-bottom: 1px solid var(--ax-border);
    color: var(--ax-text);
  }

  .query.empty {
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
  }

  .items {
    display: flex;
    flex-direction: column;
    max-height: calc(var(--rows) * (1lh + 2px));
    overflow-y: auto;
  }

  .item {
    display: flex;
    align-items: baseline;
    gap: var(--ax-space-2);
    padding: 1px var(--ax-space-2);
    background: none;
    border: 0;
    color: inherit;
    font: inherit;
    text-align: left;
    white-space: nowrap;
    cursor: pointer;
  }

  .item.chosen {
    background: var(--ax-accent-muted);
  }

  .item.disabled {
    color: var(--ax-text-muted);
    cursor: default;
  }

  .number {
    min-width: 1ch;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .kind {
    min-width: 7ch;
    color: var(--ax-accent);
    font-size: var(--ax-font-size-xs);
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .reason {
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .none {
    padding: 1px var(--ax-space-2);
    color: var(--ax-text-muted);
  }
</style>
