<!--
  The Markdown preview of the file app (`docs/plans/editor.md`, D11, G8): the
  same renderer as the FileViewer (`core/markdown.ts`), with every top-level
  block marked by its source line so the preview and the source scroll in step.

  Relative images are read through the file service with the note's own root,
  so a note inside a project finds its images, not only one in the workspace.
  Rendering waits for a short pause in typing; a big note is not re-rendered on
  every keystroke.
-->
<script lang="ts">
  import { KEEP_SCROLL } from "./keepScroll";
  import { invokeBackend } from "../core/backend";
  import { renderMarkdownBlocks } from "../core/markdown";
  import { resolveMarkdownImagesWith } from "../core/markdownImages";

  interface Props {
    /** The note's source. */
    text: string;
    root: string;
    rel: string;
    /** The first source line whose block is on screen, after every scroll. */
    onTopLine?: (line: number) => void;
  }

  let { text, root, rel, onTopLine }: Props = $props();

  /** Pause in typing before the preview renders again. */
  const RENDER_DELAY_MS = 150;

  let html = $state("");
  let scroller: HTMLDivElement;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let generation = 0;

  $effect(() => {
    const source = text;
    const at = { root, rel };
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => void render(source, at), html ? RENDER_DELAY_MS : 0);
    return () => clearTimeout(timer);
  });

  async function render(source: string, at: { root: string; rel: string }): Promise<void> {
    const mine = ++generation;
    const read = (imageRel: string) =>
      invokeBackend<{ mime: string; base64: string }>("file_read_image", { root: at.root, rel: imageRel });
    const resolved = await resolveMarkdownImagesWith(source, at.rel, read);
    if (mine === generation) html = renderMarkdownBlocks(resolved);
  }

  function blocks(): HTMLElement[] {
    return scroller ? [...scroller.querySelectorAll<HTMLElement>("[data-line]")] : [];
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

<div class="preview ax-prose" {...KEEP_SCROLL} bind:this={scroller} onscroll={onScroll}>
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
</style>
