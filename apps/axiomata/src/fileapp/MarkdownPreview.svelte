<!--
  The Markdown preview of the file app (`docs/plans/editor.md`, D11, G8): the
  same renderer as the FileViewer (`core/markdown.ts`), with every top-level
  block marked by its source line so the preview and the source scroll in step.

  Relative images are read through the file service with the note's own root,
  so a note inside a project finds its images, not only one in the workspace.
  Rendering waits for a short pause in typing; a big note is not re-rendered on
  every keystroke.

  A vault note's wikilinks and tags are links (`docs/plans/tags-links.md`,
  TL2) when `notes` is given: a click on one is handed to the owner, nothing
  navigates by itself. Its frontmatter is not drawn, so its `tags:` show as
  chips at the top (concept note Q6, 2026-10-29); a chip is a tag's click too.
-->
<script lang="ts">
  import { KEEP_SCROLL } from "./keepScroll";
  import { invokeBackend } from "../core/backend";
  import { frontmatterTags } from "../core/frontmatterTags";
  import { NOTE_HREF, renderMarkdownBlocks, TAG_HREF, type NoteLinkRendering } from "../core/markdown";
  import { resolveMarkdownImagesWith } from "../core/markdownImages";

  interface Props {
    /** The note's source. */
    text: string;
    root: string;
    rel: string;
    /** The first source line whose block is on screen, after every scroll. */
    onTopLine?: (line: number) => void;
    /** A vault note: its wikilinks and tags render as links. A new object renders again (the vault became known). */
    notes?: NoteLinkRendering | null;
    /** A click on a wikilink, with its inner text (`Target#Heading|alias`). */
    onNoteLink?: (inner: string) => void;
    /** A click on a tag, without its `#`. */
    onTag?: (tag: string) => void;
  }

  let { text, root, rel, onTopLine, notes = null, onNoteLink, onTag }: Props = $props();

  /** Pause in typing before the preview renders again. */
  const RENDER_DELAY_MS = 150;

  let html = $state("");
  /** The frontmatter's tags, for a vault note only: elsewhere a `tags:` key is no tag of the vault. */
  const chips = $derived(notes ? frontmatterTags(text) : []);
  let scroller: HTMLDivElement;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let generation = 0;

  $effect(() => {
    const source = text;
    const at = { root, rel };
    const links = notes;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => void render(source, at, links), html ? RENDER_DELAY_MS : 0);
    return () => clearTimeout(timer);
  });

  async function render(
    source: string,
    at: { root: string; rel: string },
    links: NoteLinkRendering | null,
  ): Promise<void> {
    const mine = ++generation;
    const read = (imageRel: string) =>
      invokeBackend<{ mime: string; base64: string }>("file_read_image", { root: at.root, rel: imageRel });
    const resolved = await resolveMarkdownImagesWith(source, at.rel, read);
    if (mine === generation) html = renderMarkdownBlocks(resolved, links);
  }

  /** A click on a wikilink or a tag goes to the owner; every other click is the page's as before. */
  function onClick(e: MouseEvent): void {
    const anchor = (e.target as Element | null)?.closest?.("a");
    const href = anchor?.getAttribute("href") ?? "";
    const note = href.startsWith(NOTE_HREF);
    if (!note && !href.startsWith(TAG_HREF)) return;
    e.preventDefault();
    let value: string;
    try {
      value = decodeURIComponent(href.slice(note ? NOTE_HREF.length : TAG_HREF.length));
    } catch {
      return;
    }
    if (note) onNoteLink?.(value);
    else onTag?.(value);
  }

  function blocks(): HTMLElement[] {
    return scroller ? [...scroller.querySelectorAll<HTMLElement>("[data-line]")] : [];
  }

  /** Takes the keyboard focus, so the editor's keys (⌃- back, ⌘⇧V) reach it while only the page shows. */
  export function focus(): void {
    scroller?.focus({ preventScroll: true });
  }

  /** Scrolls to the last block starting at or before `line`. */
  export function scrollToLine(line: number): void {
    let target: HTMLElement | undefined;
    for (const block of blocks()) {
      if (Number(block.dataset.line) > line) break;
      target = block;
    }
    // `.preview` is positioned, so a block's offsetTop is already relative to it.
    // The scroll event this fires is reported like any other; the owner's
    // `ScrollLink` tells it apart as an echo (H12).
    scroller.scrollTop = target ? target.offsetTop : 0;
  }

  function onScroll(): void {
    if (!onTopLine) return;
    const top = scroller.scrollTop;
    let line = 0;
    for (const block of blocks()) {
      if (block.offsetTop > top) break;
      line = Number(block.dataset.line);
    }
    onTopLine(line);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div
  class="preview ax-prose"
  tabindex="-1"
  {...KEEP_SCROLL}
  bind:this={scroller}
  onscroll={onScroll}
  onclick={onClick}
>
  {#if chips.length > 0}
    <div class="front-tags" role="group" aria-label="Tags">
      {#each chips as tag (tag)}
        <button
          type="button"
          class="chip"
          title="Show the notes tagged #{tag} in the Second Brain"
          onclick={() => onTag?.(tag)}>#{tag}</button
        >
      {/each}
    </div>
  {/if}
  {@html html}
</div>

<style>
  .preview {
    position: relative;
    height: 100%;
    overflow: auto;
    padding: var(--ax-space-4) var(--ax-space-5);
    color: var(--ax-text);
    background: var(--ax-surface-1);
    font-family: var(--ax-font-sans);
    /* Reading text one step above the base size: 14px sans read small next to the editor's text. */
    font-size: var(--ax-preview-font-size, var(--ax-font-size-lg));
    line-height: 1.55;
    user-select: text;
  }

  .preview:focus {
    outline: none;
  }

  /* The frontmatter's tags: plain chips as in the Second Brain's detail panel — no accent, a tag is not alive. */
  .front-tags {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ax-space-1);
    margin-bottom: var(--ax-space-3);
  }
  .chip {
    padding: calc(1px * var(--ax-ui-scale)) var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-2);
    color: var(--ax-text-muted);
    font: inherit;
    font-size: var(--ax-font-size-xs);
    line-height: 1.4;
    cursor: pointer;
  }
  .chip:hover,
  .chip:focus-visible {
    background: var(--ax-selected-bg);
    color: var(--ax-text);
  }

  /* A note's wikilinks (a dangling one faded) and tags (TL2): the editor's colours, no accent. */
  .preview :global(a.ax-wikilink) {
    color: var(--ax-syntax-wikilink);
  }
  .preview :global(a.ax-wikilink-missing) {
    color: var(--ax-syntax-wikilink-missing);
    text-decoration-style: dotted;
  }
  .preview :global(a.ax-tag) {
    color: var(--ax-syntax-hashtag);
    text-decoration: none;
  }
  .preview :global(a.ax-tag:hover) {
    text-decoration: underline;
  }
</style>
