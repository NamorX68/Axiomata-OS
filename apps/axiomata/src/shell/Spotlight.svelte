<!--
  The ⌘K search (`docs/plans/spotlight-search.md`): a pill in the middle of the screen, the hits listed below it. Typing
  searches the instant sources (shell actions, modules, skills, routines, cards, plans, projects); ↑/↓ only *select* a hit,
  and nothing is carried out until Return or a double click (S5). Esc or a click outside closes it and gives the focus back.
  Under the hits there is always "Ask Axiomata" (UX7a, G20): Return on it sends the query to the chat at once and opens it
  — the only row when nothing matches, so a question typed into ⌘K is one Return from an answer (the daily cap bounds
  what it costs).
-->
<script lang="ts">
  import { tick, untrack } from "svelte";

  import { get } from "svelte/store";

  import type { SearchHit } from "../core/backend";
  import { busy, send } from "../core/chat";
  import { toast } from "../core/toast";
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
  /** The question "Ask Axiomata" would send; with none there is no such row. */
  const question = $derived(query.trim());
  /** Every row the arrows walk: the hits, and "Ask Axiomata" after them (its index is `rows.length`). */
  const total = $derived(rows.length + (question ? 1 : 0));

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

  /** Sends the query to the chat, which opens with the answer coming (G20). */
  function ask(): void {
    const message = question;
    if (!message) return;
    close(false);
    if (get(busy)) {
      toast("The agent is still answering — ask again in a moment.", "warning");
      return;
    }
    void send(message, "chat");
  }

  function choose(index: number, secondary = false): void {
    if (index === rows.length) {
      ask();
      return;
    }
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
      if (total === 0) return;
      const step = event.key === "ArrowDown" ? 1 : -1;
      active = (active + step + total) % total;
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
        aria-expanded={total > 0}
        aria-controls="spotlight-list"
        aria-activedescendant={total > 0 ? `spotlight-row-${active}` : undefined}
        placeholder="Search, or ask Axiomata …"
        autocomplete="off"
        spellcheck="false"
        onkeydown={onKeydown}
      />
      {#if total > 0}
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
          {#if question}
            {#if rows.length > 0}<li class="group" role="presentation">Ask</li>{/if}
            <li
              id="spotlight-row-{rows.length}"
              class="row ask"
              role="option"
              tabindex="-1"
              aria-selected={active === rows.length}
              data-active={active === rows.length}
              onmousemove={() => (active = rows.length)}
              ondblclick={ask}
            >
              <span class="ask-mark ax-hex" aria-hidden="true"></span>
              <span class="title">Ask Axiomata</span>
              <span class="sub">“{question}”</span>
            </li>
          {/if}
        </ul>
        {#if searching}<p class="busy">Searching files …</p>{/if}
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
  .results {
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
  /* The chosen row is chosen, not alive: neutral (the accent rule). */
  .row[data-active="true"] {
    background: var(--ax-selected-bg);
  }
  /* "Ask Axiomata": the agent's hexagon, glowing — asking it starts work. */
  .ask-mark {
    flex: none;
    align-self: center;
    width: calc(12px * var(--ax-ui-scale));
    height: calc(14px * var(--ax-ui-scale));
    background: var(--ax-glow);
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
