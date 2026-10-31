<!--
  The Help panel (`docs/plans/chat-knows-app.md`, CH4, Q9): the app's handbook in the Markdown reading view, the pages
  listed on the left, a link between pages opening the other page here. Opened by the Dock's `?`, a spotlight hit in
  the "Handbook" group and the chat's `/help <topic>` — all through `shell:handbook` (`core/handbook.ts`).

  The pages come by name from the build (`handbook_pages` / `handbook_page`); the webview never names a path. Their
  Markdown is rendered by `core/markdown.ts`, which sanitises it, so `{@html}` only ever shows what DOMPurify kept.
-->
<script lang="ts">
  import { lang } from "../i18n/lang.svelte";
  import { messageOf } from "../core/errors";
  import { renderMarkdown } from "../core/markdown";
  import { listHandbook, pageOfLink, readHandbookPage, withPageLinks, type HandbookEntry } from "../core/handbook";
  import { registerShellView } from "../core/openViews";
  import { tShell } from "../i18n/shell";
  import Window from "./Window.svelte";

  let {
    open = $bindable(false),
    request = null,
  }: {
    open?: boolean;
    /** The page asked for when opened, numbered so the same page asked for twice is shown again. */
    request?: { page: string | null; seq: number } | null;
  } = $props();

  /** The size the window first opens at (unscaled px): large enough for a page beside the list. `Window` holds it to
   *  the screen; once the owner resized it, the remembered size wins (`sizeKey`). */
  const PANEL_SIZE = { w: 980, h: 720 };

  /** The overview, shown when nothing else was asked for. */
  const FIRST_PAGE = "index";

  let entries = $state<HandbookEntry[]>([]);
  let current = $state(FIRST_PAGE);
  let html = $state("");
  let failure = $state<string | null>(null);
  let article = $state<HTMLElement | null>(null);

  const known = $derived(new Set(entries.map((entry) => entry.name)));
  const title = $derived(entries.find((entry) => entry.name === current)?.title ?? tShell("handbook"));

  // Known as open to the chat's `close_view` (CH5); a page being read loses nothing by closing.
  $effect(() =>
    open
      ? registerShellView("handbook", { title: tShell("handbookView", { title }), close: () => (open = false) })
      : undefined,
  );

  /** The language the table of contents was read in: a switch reads it again (titles differ by language). */
  let entriesLang = $state<string | null>(null);

  $effect(() => {
    const language = lang();
    if (!open || entriesLang === language) return;
    listHandbook()
      .then((list) => {
        entries = list;
        entriesLang = language;
      })
      .catch((err) => (failure = messageOf(err)));
  });

  // A page asked for from outside wins over the one last shown.
  $effect(() => {
    if (open && request?.page) current = request.page;
  });

  $effect(() => {
    if (!open) return;
    const name = current;
    readHandbookPage(name)
      .then((page) => {
        if (name !== current) return;
        failure = null;
        html = renderMarkdown(withPageLinks(page.text));
        article?.scrollTo({ top: 0 });
      })
      .catch((err) => {
        if (name === current) failure = messageOf(err);
      });
  });

  /** A link to another page opens it here; every other link keeps its own behaviour. */
  function onArticleClick(event: MouseEvent): void {
    const anchor = (event.target as Element | null)?.closest?.("a");
    const target = pageOfLink(anchor?.getAttribute("href") ?? "", known);
    if (!target) return;
    event.preventDefault();
    current = target;
  }
</script>

{#if open}
  <Window
    title={tShell("handbook")}
    subtitle={title}
    onClose={() => (open = false)}
    movable
    resizable
    sizeKey="handbook"
    defaultSize={PANEL_SIZE}
  >
    <div class="handbook">
      <nav class="pages" aria-label={tShell("handbookPages")}>
        {#each entries as entry (entry.name)}
          <button
            type="button"
            class="page"
            class:selected={entry.name === current}
            aria-current={entry.name === current ? "page" : undefined}
            title={entry.summary}
            onclick={() => (current = entry.name)}
          >
            {entry.title}
          </button>
        {/each}
      </nav>
      <!-- A delegated click on the page's own links; the links stay keyboard-reachable as links. -->
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
      <article class="reading ax-prose" bind:this={article} onclick={onArticleClick}>
        {#if failure}
          <p class="failure">{failure}</p>
        {:else}
          {@html html}
        {/if}
      </article>
    </div>
  </Window>
{/if}

<style>
  .handbook {
    flex: 1 1 auto;
    min-height: 0;
    display: grid;
    grid-template-columns: calc(200px * var(--ax-ui-scale)) 1fr;
  }

  .pages {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    padding: var(--ax-space-2);
    overflow: auto;
    border-right: 1px solid var(--ax-border);
  }

  .page {
    text-align: left;
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 0;
    border-radius: var(--ax-radius-sm);
    background: transparent;
    color: var(--ax-text-muted);
    font: var(--ax-type-body);
    cursor: pointer;
  }

  .page:hover {
    color: var(--ax-text);
  }

  /* Selected is not alive: the quiet selection ground, never the accent. */
  .page.selected {
    background: var(--ax-selected-bg);
    color: var(--ax-text);
  }

  .reading {
    min-height: 0;
    overflow: auto;
    padding: var(--ax-space-3) var(--ax-space-5) var(--ax-space-5);
  }

  .failure {
    color: var(--ax-text-muted);
    font: var(--ax-type-body);
  }

  /* A phone-narrow window: the list goes above the page. */
  @media (max-width: 640px) {
    .handbook {
      grid-template-columns: 1fr;
      grid-template-rows: auto 1fr;
    }

    .pages {
      flex-direction: row;
      flex-wrap: wrap;
      border-right: 0;
      border-bottom: 1px solid var(--ax-border);
    }
  }
</style>
