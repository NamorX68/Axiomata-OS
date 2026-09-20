<script lang="ts">
  /**
   * kanban — a board of columns and cards.
   *
   * One module, three shapes, chosen by the instance config rather than by
   * three registrations: a canvas tile to glance at, the same board as a
   * floating panel to work in (`openStaged` with `path: "board:<id>"`), and a
   * single card's detail (`path: "card:<id>"`). They share one store per
   * board, so a change in one is a change in all of them.
   *
   * Read-only for now — CP-K2a. Dragging, quick-add, editing and column
   * management arrive with CP-K2b; the layout is built to receive them.
   *
   * Appearance follows the decisions taken by looking at a throwaway preview
   * in every theme (CP-K2-Design): the column body is `--ax-bg`, the desk the
   * cards lie on, and the card itself asks the theme how it should sit via
   * `--ax-card-*`. Only labels carry colour. Nothing here is framed — the
   * tile's front face is frameless, and a column drawn with a border would
   * fight that.
   */
  import { onMount } from "svelte";

  import type { BoardCard } from "../core/backend";
  import { invokeBackend as invoke } from "../core/backend";
  import { boardStore } from "../core/boardStore";
  import {
    actorLabel,
    applyFilter,
    collectLabels,
    dueState,
    groupByColumn,
    labelColorIndex,
    showsAssignee,
    type CardFilter,
  } from "../core/kanban";
  import { hostAnchor, openStaged } from "../core/staging";
  import type { ModuleContext } from "../core/types";

  let { ctx }: { ctx: ModuleContext } = $props();
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  /** Label colours live as tokens so each theme can tune them. */
  const LABEL_TOKENS = 6;
  /** D1: columns squeeze to here, then the board scrolls. Never hidden. */
  const MIN_COL_PX = 140;

  const cardId = $derived(typeof $config.cardId === "number" ? $config.cardId : null);
  const staged = $derived($config.path !== undefined);

  let boardId = $state<number | null>(null);
  let listError = $state("");
  let filterText = $state("");
  let activeLabels = $state<string[]>([]);
  let showArchived = $state(false);

  // A board id in the config wins; otherwise the first board there is. A board
  // is only created explicitly (flip side), never implicitly by looking.
  onMount(async () => {
    const configured = typeof $config.boardId === "number" ? $config.boardId : null;
    if (configured !== null) {
      boardId = configured;
      return;
    }
    try {
      const boards = await invoke<{ id: number }[]>("list_boards");
      boardId = boards[0]?.id ?? null;
    } catch (err) {
      listError = String(err);
    }
  });

  const data = $derived(boardId === null ? null : boardStore(boardId));

  const filter = $derived<CardFilter>({
    text: filterText,
    labels: activeLabels,
    includeArchived: showArchived,
  });

  const visible = $derived($data ? applyFilter($data.cards, filter) : []);
  const grouped = $derived($data ? groupByColumn($data.columns, visible) : []);
  const allLabels = $derived($data ? collectLabels($data.cards) : []);
  const boardEmpty = $derived($data !== null && $data.cards.length === 0);
  const detail = $derived<BoardCard | null>(
    cardId === null ? null : ($data?.cards.find((card) => card.id === cardId) ?? null),
  );

  function openCard(card: BoardCard, event: MouseEvent) {
    // `openStaged` recognises an already-open panel only by `config.path`
    // (core/staging.ts), so the card id goes in as one — without it every
    // click would stack another copy of the same card.
    //
    // The anchor makes the detail appear over the tile the card was clicked
    // in, rather than in the middle of the screen: the eye is already there.
    openStaged("kanban", {
      path: `card:${card.id}`,
      cardId: card.id,
      boardId: card.board_id,
      anchor: hostAnchor(event.currentTarget as Element),
    });
  }

  function toggleLabel(label: string) {
    activeLabels = activeLabels.includes(label)
      ? activeLabels.filter((l) => l !== label)
      : [...activeLabels, label];
  }
</script>

{#if listError}
  <p class="notice danger">{listError}</p>
{:else if boardId === null}
  <p class="notice">Noch kein Brett. Leg auf der Rückseite eines an.</p>
{:else if $data === null || ($data.loading && $data.board === null)}
  <p class="notice">Lädt …</p>
{:else if $data.error}
  <p class="notice danger">{$data.error}</p>
{:else if cardId !== null}
  <!-- Detail of a single card. -->
  {#if detail === null}
    <p class="notice">Diese Karte gibt es nicht mehr.</p>
  {:else}
    <article class="detail">
      <h2>{detail.title}</h2>
      {#if detail.labels.length > 0}
        <div class="chips">
          {#each detail.labels as label (label)}
            <span class="chip" data-tone={labelColorIndex(label, LABEL_TOKENS)}>{label}</span>
          {/each}
        </div>
      {/if}
      {#if detail.body}
        <p class="body">{detail.body}</p>
      {:else}
        <p class="body muted">Kein Text.</p>
      {/if}
      <dl>
        {#if detail.due_at}
          {@const due = dueState(detail.due_at)}
          <dt>Fällig</dt>
          <dd class:over={due.overdue}>{due.label}</dd>
        {/if}
        {#if showsAssignee(detail.assignee)}
          <dt>Zuständig</dt>
          <dd>{actorLabel(detail.assignee)}</dd>
        {/if}
        {#if detail.claimed_by}
          <dt>Übernommen</dt>
          <dd>{actorLabel(detail.claimed_by)}</dd>
        {/if}
        {#if detail.verified_by}
          <dt>Abgenommen</dt>
          <dd>✓ {actorLabel(detail.verified_by)}</dd>
        {/if}
        {#if detail.archived_at}
          <dt>Archiviert</dt>
          <dd>ja</dd>
        {/if}
      </dl>
    </article>
  {/if}
{:else}
  <div class="kanban">
    {#if staged}
      <!-- Filters live in the panel only: the tile is for glancing at. -->
      <div class="filters">
        <input
          type="search"
          placeholder="Karten filtern …"
          bind:value={filterText}
          aria-label="Karten filtern"
        />
        {#each allLabels as label (label)}
          <button
            class="chip"
            data-tone={labelColorIndex(label, LABEL_TOKENS)}
            class:on={activeLabels.includes(label)}
            onclick={() => toggleLabel(label)}
          >
            {label}
          </button>
        {/each}
        <label class="archived">
          <input type="checkbox" bind:checked={showArchived} />
          Archiv
        </label>
      </div>
    {/if}

    <div class="board" style="--min-col: {MIN_COL_PX}px">
      {#each grouped as { column, cards } (column.id)}
        <section class="col">
          <header>
            <span class="name">{column.name}</span>
            <span class="count">{cards.length}</span>
          </header>
          <div class="cards">
            {#each cards as card (card.id)}
              <article class="card" class:archived={card.archived_at !== null}>
                <button class="open" onclick={(event) => openCard(card, event)}>
                  <span class="title">{card.title}</span>
                </button>
                {#if card.body}
                  <p class="body">{card.body}</p>
                {/if}
                {#if card.labels.length > 0 || card.due_at || card.verified_by}
                  <div class="foot">
                    <span class="dots">
                      {#each card.labels as label (label)}
                        <span
                          class="dot"
                          data-tone={labelColorIndex(label, LABEL_TOKENS)}
                          title={label}
                        ></span>
                      {/each}
                    </span>
                    <span class="chips">
                      {#each card.labels as label (label)}
                        <span class="chip" data-tone={labelColorIndex(label, LABEL_TOKENS)}>
                          {label}
                        </span>
                      {/each}
                    </span>
                    {#if card.verified_by}
                      <span class="verified" title="abgenommen von {actorLabel(card.verified_by)}"
                        >✓</span
                      >
                    {/if}
                    {#if card.due_at}
                      {@const due = dueState(card.due_at)}
                      <span class="due" class:over={due.overdue}>{due.label}</span>
                    {/if}
                  </div>
                {/if}
                {#if showsAssignee(card.assignee)}
                  <p class="who">{actorLabel(card.assignee)}</p>
                {/if}
              </article>
            {:else}
              <p class="empty">Keine Karten</p>
            {/each}
          </div>
        </section>
      {/each}
    </div>

    {#if boardEmpty}
      <p class="notice">
        Noch nichts hier. Karten legst du derzeit über
        <code>axiomata-cli board add</code> an.
      </p>
    {/if}
  </div>
{/if}

<style>
  .kanban {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    gap: var(--ax-space-2);
    padding: var(--ax-space-2);
    font-family: var(--ax-font-sans);
    color: var(--ax-text);
  }

  .notice {
    margin: 0;
    padding: var(--ax-space-3);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
    text-align: center;
  }
  .notice.danger {
    color: var(--ax-danger);
    text-align: left;
  }
  .notice code {
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--ax-space-2);
  }
  .filters input[type="search"] {
    flex: 1 1 160px;
    min-width: 0;
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    color: var(--ax-text);
    font: inherit;
    font-size: var(--ax-font-size-sm);
  }
  .archived {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  /* D1: squeeze to --min-col, then scroll sideways. A board that hides a
     column lies about the state of the work. */
  .board {
    display: flex;
    gap: var(--ax-space-2);
    flex: 1 1 auto;
    min-height: 0;
    overflow-x: auto;
    overflow-y: hidden;
  }
  .col {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1 1 0;
    min-width: var(--min-col);
    /* D3: the desk the cards lie on — a tint, never a frame. */
    background: var(--ax-bg);
    border-radius: var(--ax-radius-md);
    padding: var(--ax-space-2);
    container: col / inline-size;
  }

  header {
    display: flex;
    align-items: baseline;
    gap: var(--ax-space-2);
    padding: 0 var(--ax-space-1) var(--ax-space-2);
  }
  .name {
    font-size: var(--ax-font-size-sm);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .count {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    font-family: var(--ax-font-mono);
  }

  .cards {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    overflow-y: auto;
    min-height: 0;
    flex: 1 1 auto;
  }

  /* The theme decides how a card sits on its surface (CP-K2-Design). */
  .card {
    background: var(--ax-card-bg);
    border: 1px solid var(--ax-card-border);
    box-shadow: var(--ax-card-shadow);
    border-radius: var(--ax-radius-md);
    padding: var(--ax-space-2);
  }
  .card.archived {
    opacity: 0.55;
  }

  .open {
    display: block;
    width: 100%;
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .open:focus-visible {
    outline: 2px solid var(--ax-focus-ring);
    outline-offset: 2px;
    border-radius: var(--ax-radius-sm);
  }
  /* One step up the scale rather than a literal 13px: the scale runs
     11/12/14/16/20, and wedging a 13 between 12 and 14 would give four steps
     inside three pixels, which stops being a scale. The meta row below stays
     small on purpose — the gap is what makes the title read as the title. */
  .title {
    font-size: var(--ax-font-size-base);
    line-height: var(--ax-line-height);
  }
  .card .body {
    margin: var(--ax-space-1) 0 0;
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
    line-height: var(--ax-line-height);
  }

  .foot {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--ax-space-1) var(--ax-space-2);
    margin-top: var(--ax-space-2);
    font-size: var(--ax-font-size-xs);
  }
  .dots {
    display: none;
    gap: 3px;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: var(--ax-radius-pill);
    background: var(--tone);
  }
  .chips {
    display: inline-flex;
    flex-wrap: wrap;
    gap: var(--ax-space-1);
  }
  /* D2: labels are the only thing on the board that carries colour. */
  .chip {
    padding: 0 6px;
    border: 1px solid var(--tone);
    border-radius: var(--ax-radius-pill);
    background: transparent;
    color: var(--tone);
    font: inherit;
    font-size: var(--ax-font-size-xs);
    line-height: 1.6;
  }
  button.chip {
    cursor: pointer;
  }
  button.chip.on {
    background: var(--ax-accent-muted);
  }
  [data-tone="0"] {
    --tone: var(--ax-label-1);
  }
  [data-tone="1"] {
    --tone: var(--ax-label-2);
  }
  [data-tone="2"] {
    --tone: var(--ax-label-3);
  }
  [data-tone="3"] {
    --tone: var(--ax-label-4);
  }
  [data-tone="4"] {
    --tone: var(--ax-label-5);
  }
  [data-tone="5"] {
    --tone: var(--ax-label-6);
  }

  .verified {
    color: var(--ax-success);
  }
  .due {
    margin-left: auto;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
    white-space: nowrap;
  }
  .due.over {
    color: var(--ax-danger);
  }
  .who {
    margin: var(--ax-space-1) 0 0;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .empty {
    margin: var(--ax-space-2) var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  /* Density follows the COLUMN's width, not the board's: a wide board with
     eight columns still has narrow columns. */
  @container col (max-width: 200px) {
    .chips,
    .card .body,
    .who {
      display: none;
    }
    .dots {
      display: inline-flex;
    }
  }

  /* ------------------------------------------------------------ detail --- */

  .detail {
    padding: var(--ax-space-4);
    font-family: var(--ax-font-sans);
    color: var(--ax-text);
    overflow-y: auto;
    height: 100%;
  }
  .detail h2 {
    margin: 0 0 var(--ax-space-3);
    font-size: var(--ax-font-size-lg);
    line-height: var(--ax-line-height);
  }
  .detail .chips {
    margin-bottom: var(--ax-space-3);
  }
  .detail .body {
    margin: 0 0 var(--ax-space-4);
    font-size: var(--ax-font-size-sm);
    line-height: var(--ax-line-height);
    white-space: pre-wrap;
  }
  .detail .body.muted {
    color: var(--ax-text-muted);
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--ax-space-1) var(--ax-space-3);
    margin: 0;
    font-size: var(--ax-font-size-sm);
  }
  dt {
    color: var(--ax-text-muted);
  }
  dd {
    margin: 0;
  }
  dd.over {
    color: var(--ax-danger);
  }
</style>
