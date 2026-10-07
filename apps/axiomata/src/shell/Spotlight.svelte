<!--
  The ⌘K search (`docs/plans/spotlight-search.md`): a pill in the middle of the screen, the hits listed below it. Typing
  searches the instant sources (shell actions, modules, skills, routines, cards, plans, projects); ↑/↓ only *select* a hit,
  and nothing is carried out until Return or a double click (S5). Esc or a click outside closes it and gives the focus back.
-->
<script lang="ts">
  import { tick, untrack } from "svelte";

  import type { SearchHit } from "../core/backend";
  import { GROUP_LABEL, rankItems, type SpotlightSources } from "../core/spotlight";
  import { withContentHits, type SpotlightAction } from "../core/spotlightItems";
  import { loadInstantSources, runSpotlightAction, searchFileContents } from "../core/spotlightRun";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  /** A query shorter than this is not sent to the full-text search. */
  const MIN_CONTENT_QUERY = 2;
  const CONTENT_DEBOUNCE_MS = 180;

  let query = $state("");
  let sources = $state<SpotlightSources>({});
  let active = $state(0);
  let input = $state<HTMLInputElement | null>(null);
  let list = $state<HTMLUListElement | null>(null);
  let opener: Element | null = null;
  let wasOpen = false;

  /** The files the full-text search found for the current query, and whether that search is still on its way. */
  let contentHits = $state<SearchHit[]>([]);
  let searching = $state(false);
  let requestId = 0;

  const rows = $derived(rankItems(query, { ...sources, file: withContentHits(sources.file ?? [], contentHits) }));

  /** A new list starts at its first hit. */
  $effect(() => {
    void rows;
    active = 0;
  });

  $effect(() => {
    if (open === wasOpen) return;
    wasOpen = open;
    if (open) untrack(onOpen);
  });

  /**
   * The content search: a round-trip, so debounced, and a slow answer to an old query is dropped. Short queries match
   * too much to be worth it.
   */
  $effect(() => {
    const wanted = query.trim();
    if (!open || wanted.length < MIN_CONTENT_QUERY) {
      contentHits = [];
      searching = false;
      return;
    }
    const mine = ++requestId;
    searching = true;
    const timer = setTimeout(() => {
      searchFileContents(wanted)
        .then((hits) => {
          if (mine === requestId) contentHits = hits;
        })
        .catch(() => {
          if (mine === requestId) contentHits = [];
        })
        .finally(() => {
          if (mine === requestId) searching = false;
        });
    }, CONTENT_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  });

  /** Keeps the selected hit in view while ↑/↓ walk a list longer than the panel. */
  $effect(() => {
    void active;
    list?.querySelector('[data-active="true"]')?.scrollIntoView({ block: "nearest" });
  });

  function onOpen(): void {
    opener = document.activeElement;
    query = "";
    active = 0;
    // Fresh each time: a card or skill made since the last look is in the list at once.
    void loadInstantSources().then((loaded) => (sources = loaded));
    void tick().then(() => input?.focus());
  }

  /** Closes and gives the focus back to what had it — unless a hit was chosen, whose action takes it elsewhere. */
  function close(restoreFocus = true): void {
    open = false;
    if (restoreFocus && opener instanceof HTMLElement) opener.focus();
    opener = null;
  }

  function choose(index: number, secondary = false): void {
    const row = rows[index];
    if (!row) return;
    close(false);
    void runSpotlightAction(row.item.payload as SpotlightAction, secondary);
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      // Not for the Escape chain behind it (the staged panels, the Second Brain): this is the topmost thing.
      event.preventDefault();
      event.stopPropagation();
      close();
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (rows.length === 0) return;
      const step = event.key === "ArrowDown" ? 1 : -1;
      active = (active + step + rows.length) % rows.length;
    } else if (event.key === "Enter") {
      event.preventDefault();
      choose(active, event.metaKey || event.ctrlKey);
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="scrim" onpointerdown={(event) => event.target === event.currentTarget && close()}>
    <div class="spot" role="dialog" aria-modal="true" aria-label="Search">
      <input
        bind:this={input}
        bind:value={query}
        class="pill"
        type="text"
        role="combobox"
        aria-label="Search"
        aria-expanded={rows.length > 0}
        aria-controls="spotlight-list"
        aria-activedescendant={rows.length > 0 ? `spotlight-row-${active}` : undefined}
        placeholder="Search …"
        autocomplete="off"
        spellcheck="false"
        onkeydown={onKeydown}
      />
      {#if rows.length > 0}
        <ul id="spotlight-list" class="results" role="listbox" aria-label="Results" bind:this={list}>
          {#each rows as row, index (row.group + ":" + row.item.id)}
            {#if row.groupStart}
              <li class="group" role="presentation">{GROUP_LABEL[row.group]}</li>
            {/if}
            <li
              id="spotlight-row-{index}"
              class="row"
              role="option"
              tabindex="-1"
              aria-selected={index === active}
              data-active={index === active}
              onmousemove={() => (active = index)}
              ondblclick={() => choose(index)}
            >
              <span class="title">{row.item.title}</span>
              {#if row.item.detail ?? row.item.subtitle}<span class="sub">{row.item.detail ?? row.item.subtitle}</span>{/if}
            </li>
          {/each}
        </ul>
        {#if searching}<p class="busy">Searching files …</p>{/if}
      {:else if query.trim() !== ""}
        <p class="none">{searching ? "Searching files …" : "No results"}</p>
      {/if}
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: var(--ax-z-dialog);
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 22vh;
    background: var(--ax-overlay);
  }
  .spot {
    width: min(calc(640px * var(--ax-ui-scale)), calc(100vw - 2 * var(--ax-space-4)));
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
  }
  .spot .pill {
    height: calc(52px * var(--ax-ui-scale));
    padding: 0 calc(22px * var(--ax-ui-scale));
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-1);
    color: var(--ax-text);
    font: inherit;
    font-size: var(--ax-font-size-lg);
    box-shadow: var(--ax-shadow-pop);
    outline: none;
  }
  .spot .pill:focus-visible {
    border-color: var(--ax-accent);
    box-shadow: var(--ax-shadow-pop), var(--ax-focus-ring);
  }
  .results,
  .none {
    margin: 0;
    padding: var(--ax-space-2);
    max-height: min(60vh, calc(440px * var(--ax-ui-scale)));
    overflow-y: auto;
    list-style: none;
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-lg);
    background: var(--ax-surface-1);
    box-shadow: var(--ax-shadow-pop);
  }
  .none {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
    padding: var(--ax-space-3) var(--ax-space-4);
  }
  .busy {
    margin: 0;
    padding: 0 var(--ax-space-4);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  .group {
    padding: var(--ax-space-2) var(--ax-space-3) var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
  }
  .row {
    display: flex;
    align-items: baseline;
    gap: var(--ax-space-3);
    padding: var(--ax-space-2) var(--ax-space-3);
    border-radius: var(--ax-radius-md);
    cursor: default;
  }
  .row[data-active="true"] {
    background: var(--ax-accent-muted);
  }
  .title {
    flex: 0 0 auto;
    max-width: 60%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
</style>
