<!--
  A small look into a file (`docs/plans/editor.md`, ED4, W5, W15) — the Second
  Brain's detail panel: Markdown rendered as in the editor's preview, code and
  HTML as a read-only editor surface with its colours, a raster image as the
  picture. Only the first lines of a long file are read into it; "Open" (the
  owner's button) is the way to the whole file.

  Read-only on purpose and without Vi: a glance, not a place to type.
-->
<script lang="ts">
  import { invokeBackend, type TextFile } from "../core/backend";
  import { messageOf } from "../core/errors";
  import { EditorDocument } from "../editor/document";
  import type { SyntaxHighlighter } from "../editor/syntax/highlighter";
  import { editorFace } from "./editorFace.svelte";
  import { editorSettings } from "./editorSettings";
  import EditorSurface from "./EditorSurface.svelte";
  import { headOf, isImagePath, previewKindFor } from "./fileKinds";
  import { highlightFor } from "./highlighting";
  import MarkdownPreview from "./MarkdownPreview.svelte";
  import { NO_EFFECTS, surfaceSettings } from "./surfaceSettings";

  interface Props {
    root: string;
    rel: string;
    /** How much of a long file is shown (W15). */
    maxLines?: number;
  }

  let { root, rel, maxLines = 200 }: Props = $props();

  type Peek =
    | { kind: "loading" }
    | { kind: "error"; message: string }
    | { kind: "image"; src: string }
    | { kind: "markdown"; text: string; cut: boolean }
    | { kind: "code"; doc: EditorDocument; cut: boolean };

  let peek = $state.raw<Peek>({ kind: "loading" });
  let highlighter = $state.raw<SyntaxHighlighter | null>(null);
  /** An injected grammar arrived: the surface redraws its colours. */
  let coloured = $state(0);

  const face = editorFace();
  const settings = $derived({
    ...surfaceSettings($editorSettings, true),
    fontFamily: face.family,
    fontWeight: face.weight,
    effects: { ...NO_EFFECTS, bracketColors: $editorSettings.bracketColors },
    // No cursor to count from: plain line numbers.
    lineNumbers: "absolute" as const,
    vi: false,
    // A glance at a file: no room for the minimap, and no scrolling that would pin headers.
    minimap: false,
    stickyScroll: false,
  });

  $effect(() => {
    const at = { root, rel };
    const limit = maxLines;
    let cancelled = false;
    peek = { kind: "loading" };
    void load(at, limit, () => cancelled);
    return () => {
      cancelled = true;
      highlighter?.dispose();
      highlighter = null;
    };
  });

  async function load(at: { root: string; rel: string }, limit: number, cancelled: () => boolean): Promise<void> {
    try {
      if (isImagePath(at.rel)) {
        const image = await invokeBackend<{ mime: string; base64: string }>("file_read_image", at);
        if (!cancelled()) peek = { kind: "image", src: `data:${image.mime};base64,${image.base64}` };
        return;
      }
      const file = await invokeBackend<TextFile>("file_read", at);
      if (cancelled()) return;
      const head = headOf(file.content, limit);
      if (previewKindFor(at.rel) === "markdown") {
        peek = { kind: "markdown", text: head.text, cut: head.cut };
        return;
      }
      const doc = new EditorDocument(head.text, { indentFallback: { kind: "spaces", size: 4 } });
      peek = { kind: "code", doc, cut: head.cut };
      const created = await highlightFor(doc, { fileName: at.rel, onColours: () => coloured++, stale: cancelled });
      if (created) highlighter = created;
    } catch (err) {
      if (!cancelled()) peek = { kind: "error", message: messageOf(err) };
    }
  }
</script>

<div class="peek" class:plain={peek.kind === "loading" || peek.kind === "error"}>
  {#if peek.kind === "loading"}
    <span class="note">Loading preview…</span>
  {:else if peek.kind === "error"}
    <span class="note">Preview unavailable: {peek.message}</span>
  {:else if peek.kind === "image"}
    <div class="image"><img src={peek.src} alt={rel} /></div>
  {:else if peek.kind === "markdown"}
    <MarkdownPreview text={peek.text} {root} {rel} />
  {:else}
    {#key peek.doc}
      <EditorSurface doc={peek.doc} {settings} fileName={rel} readOnly {highlighter} revision={coloured} />
    {/key}
  {/if}
  {#if (peek.kind === "markdown" || peek.kind === "code") && peek.cut}
    <span class="cut">The first {maxLines} lines — Open shows all of it.</span>
  {/if}
</div>

<style>
  .peek {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    overflow: hidden;
  }

  .peek.plain {
    justify-content: center;
    align-items: center;
  }

  .peek > :global(.surface),
  .peek > :global(.preview) {
    flex: 1;
    min-height: 0;
  }

  .note {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }

  .image {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 0;
    padding: var(--ax-space-3);
  }

  .image img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }

  .cut {
    padding: var(--ax-space-1) var(--ax-space-3);
    border-top: 1px solid var(--ax-border);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
</style>
