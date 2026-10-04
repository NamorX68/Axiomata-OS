<!--
  The completion menu (`docs/plans/editor.md`, ED6.4): the offers below the
  word being typed, the chosen one highlighted, and beside it what the server
  says about it (its documentation, rendered through `renderMarkdown` —
  foreign text, sanitised). What it does is `completionMenu.ts`'s; this only
  draws it. A click takes an item; the mouse never takes the focus from the
  text.
-->
<script lang="ts">
  import { kindTag, type CompletionItem } from "../editor/lsp/completion";

  interface Props {
    items: CompletionItem[];
    selected: number;
    /** Where the menu's top-left corner goes, in the surface's pixels. */
    x: number;
    y: number;
    /** The chosen item's detail and documentation (sanitised HTML), once known. */
    detail: string;
    docHtml: string | null;
    onPick: (index: number) => void;
  }

  let { items, selected, x, y, detail, docHtml, onPick }: Props = $props();

  /** Rows visible at once; the list scrolls to keep the chosen one in view. */
  const VISIBLE_ROWS = 10;

  let list = $state<HTMLDivElement | null>(null);

  $effect(() => {
    const row = list?.children[selected] as HTMLElement | undefined;
    row?.scrollIntoView({ block: "nearest" });
  });
</script>

<div class="completion" style:left="{x}px" style:top="{y}px" role="presentation" onmousedown={(e) => e.preventDefault()}>
  <div class="items" role="listbox" aria-label="Suggestions" bind:this={list} style:--rows={VISIBLE_ROWS}>
    {#each items as item, i (i)}
      <button
        type="button"
        class="item"
        class:chosen={i === selected}
        role="option"
        aria-selected={i === selected}
        tabindex="-1"
        onclick={() => onPick(i)}
      >
        <span class="kind">{kindTag(item.kind)}</span>
        <span class="label">{item.label}</span>
        <span class="detail">{item.detail}</span>
      </button>
    {/each}
  </div>
  {#if detail || docHtml}
    <div class="doc">
      {#if detail}<pre class="signature">{detail}</pre>{/if}
      {#if docHtml}
        <!-- Sanitised by `renderMarkdown` (DOMPurify): the text comes from the language server. -->
        <div class="doc-text">{@html docHtml}</div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .completion {
    position: absolute;
    z-index: 7;
    display: flex;
    align-items: flex-start;
    gap: var(--ax-space-1);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text);
  }

  .items,
  .doc {
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    box-shadow: var(--ax-shadow-pop);
  }

  .items {
    display: flex;
    flex-direction: column;
    min-width: 24ch;
    max-width: 60ch;
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

  .kind {
    min-width: 4ch;
    color: var(--ax-accent);
    font-size: var(--ax-font-size-xs);
  }

  .label {
    flex: none;
  }

  .detail {
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .doc {
    max-width: 60ch;
    max-height: 40vh;
    overflow: auto;
    padding: var(--ax-space-1) var(--ax-space-2);
    font-family: var(--ax-font-sans);
  }

  .signature {
    margin: 0;
    font-family: var(--ax-font-mono);
    white-space: pre-wrap;
  }

  .signature + .doc-text {
    margin-top: var(--ax-space-2);
    padding-top: var(--ax-space-2);
    border-top: 1px solid var(--ax-border);
  }

  .doc-text :global(pre) {
    margin: var(--ax-space-1) 0;
    font-family: var(--ax-font-mono);
    white-space: pre-wrap;
  }

  .doc-text :global(p) {
    margin: var(--ax-space-1) 0;
  }
</style>
