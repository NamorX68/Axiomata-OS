<!--
  The panel above the chat bar (`docs/plans/i18n.md`, #109, Q6–Q9): a `/` command's list or answer, never written
  into the conversation. The bar keeps the keyboard — ↑↓ move `picked`, ⏎ takes, Esc closes — and hands this the
  rows it shows; a click takes a row too. A row that asks first (a skill run) shows its question in place.
-->
<script lang="ts">
  import { tChat } from "../i18n/chat";
  import IconButton from "../ui/IconButton.svelte";
  import type { Panel, PanelRow } from "../core/slashLists";

  interface Props {
    panel: Panel;
    /** The rows to show: the panel's, filtered by what is typed. */
    rows: PanelRow[];
    /** What is typed after the command, for "Nothing matches". */
    filter: string;
    picked: number;
    /** The row whose question is open. */
    confirming: string | null;
    onPick: (row: PanelRow) => void;
    onClose: () => void;
  }

  let { panel, rows, filter, picked, confirming, onPick, onClose }: Props = $props();

  let list = $state<HTMLElement | undefined>();

  /** The first column wider: it names the row (a model, a skill, a command). */
  const columns = $derived(
    panel.columns.length > 1 ? `minmax(0, 2fr) repeat(${panel.columns.length - 1}, minmax(0, 1fr))` : "minmax(0, 1fr)",
  );

  // The picked row stays in view while ↑↓ walk a list longer than the panel.
  $effect(() => {
    void picked;
    list?.querySelector(".picked")?.scrollIntoView({ block: "nearest" });
  });
</script>

<section class="panel" aria-label={panel.title}>
  <header>
    <span class="title">{panel.title}</span>
    {#if panel.note}<span class="note">{panel.note}</span>{/if}
    <IconButton icon="x" size="sm" label={tChat("closePanel")} onclick={onClose} />
  </header>

  {#if panel.text !== undefined}
    {#if panel.code}<pre>{panel.text}</pre>{:else}<p class="text">{panel.text}</p>{/if}
  {:else if rows.length === 0}
    <p class="empty">{tChat("noMatch", { query: filter })}</p>
  {:else}
    <div
      class="table"
      id="assistant-panel"
      role={panel.kind === "list" ? "listbox" : "table"}
      aria-label={panel.title}
      bind:this={list}
    >
      {#if panel.columns.length > 1}
        <div
          class="row head"
          role={panel.kind === "list" ? "presentation" : "row"}
          aria-hidden={panel.kind === "list" ? "true" : undefined}
          style:grid-template-columns={columns}
        >
          {#each panel.columns as column (column)}
            <span role={panel.kind === "list" ? undefined : "columnheader"}>{column}</span>
          {/each}
        </div>
      {/if}
      {#each rows as row, index (row.id)}
        {#if row.group && row.group !== rows[index - 1]?.group}
          <div class="group" role="presentation">{row.group}</div>
        {/if}
        {#if panel.kind === "list" && row.line}
          <button
            type="button"
            class="row"
            class:picked={index === picked}
            id="assistant-panel-row-{index}"
            tabindex="-1"
            role="option"
            aria-selected={index === picked}
            style:grid-template-columns={columns}
            onmousedown={(e) => {
              e.preventDefault();
              onPick(row);
            }}
          >
            {#if confirming === row.id && row.confirm}
              <span class="ask">{row.confirm}</span>
            {:else}
              {#each row.cells as cell, at (at)}
                <span class:first={at === 0}>{cell}{#if at === 0 && row.current}<em>{tChat("current")}</em>{/if}</span>
              {/each}
            {/if}
          </button>
        {:else}
          <div
            class="row info"
            role={panel.kind === "list" ? "presentation" : "row"}
            style:grid-template-columns={columns}
          >
            {#each row.cells as cell, at (at)}
              <span class:first={at === 0} role={panel.kind === "list" ? undefined : "cell"}>{cell}</span>
            {/each}
          </div>
        {/if}
      {/each}
    </div>
  {/if}

  {#if panel.kind === "list"}<footer>{tChat("panelHint")}</footer>{/if}
</section>

<style>
  /* Where the suggestion list opens and drawn like it (`AssistantBar.svelte`), a little wider for columns. */
  .panel {
    position: absolute;
    left: 0;
    right: 0;
    bottom: calc(100% + var(--ax-space-2));
    display: flex;
    flex-direction: column;
    padding: var(--ax-space-1);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    background: var(--ax-surface-1);
    box-shadow: var(--ax-shadow-pop);
    z-index: var(--ax-z-popover);
    font-size: var(--ax-font-size-sm);
  }

  header {
    display: flex;
    align-items: baseline;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-2);
  }

  .title {
    font-weight: 600;
    white-space: nowrap;
  }

  .note {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  /* About eight rows show; a longer list scrolls (Q8). */
  .table {
    max-height: calc(280px * var(--ax-ui-scale));
    overflow-y: auto;
  }

  .row {
    display: grid;
    gap: var(--ax-space-3);
    width: 100%;
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 0;
    border-radius: var(--ax-radius-sm);
    background: none;
    color: var(--ax-text-muted);
    font: inherit;
    text-align: left;
  }

  button.row {
    cursor: pointer;
  }

  .row span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row .first {
    color: var(--ax-text);
  }

  .row.head {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  /* Selected ≠ alive (accent rule): the picked row is a selection, not orange. */
  .row.picked {
    background: var(--ax-selected-bg);
  }

  em {
    margin-left: var(--ax-space-2);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    font-style: normal;
  }

  .ask {
    grid-column: 1 / -1;
    color: var(--ax-text);
  }

  .group {
    padding: var(--ax-space-2) var(--ax-space-2) var(--ax-space-1);
    color: var(--ax-text-muted);
    font: var(--ax-type-label);
  }

  .text,
  .empty {
    margin: 0;
    padding: var(--ax-space-1) var(--ax-space-2);
    color: var(--ax-text-muted);
  }

  pre {
    max-height: calc(280px * var(--ax-ui-scale));
    overflow: auto;
    margin: 0;
    padding: var(--ax-space-2);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  footer {
    padding: var(--ax-space-1) var(--ax-space-2) 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
</style>
