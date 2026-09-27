<!--
  A diff drawn on the editor (`docs/plans/git-layer.md`, H1–H4, H9, H11, H12):
  one read-only surface for the unified layout, or two scrolled in step for the
  split one. Shared by the IDE's Diffs tab and the file app's "Show difference"
  (H7), so what a diff looks like is decided once.

  * **The model is the owner's**, and so are the whole texts: this component
    turns them into pane documents, colours each side with its own highlighter,
    and redraws when the model changes (`revision`) or a fold is opened here.
  * **Keys**: ⌥↓/⌥↑ go to the next/previous hunk, ⏎ asks the owner to open the
    changed file at the cursor's line; every other key the owner may take first
    (`interceptKey`), the rest is the surface's (moving, selecting, copying).
  * **Folding (ED5.5, T17)** by indentation, with one `FoldState` for both panes
    of the split layout: its rows are paired, so a fold hides the same rows on
    each side and the two stay in step. Rebuilt panes (a gap opened) start unfolded.
    A fold is taken from the pane it was made in: right at a hunk boundary it may
    hide rows on the other side that are no block there (known, accepted).
-->
<script lang="ts">
  import { EditorDocument } from "../editor/document";
  import { FoldState } from "../editor/fold/state";
  import type { ViEffect } from "../editor/vi/machine";
  import { pos } from "../editor/position";
  import { DiffHighlight } from "../editor/diff/highlight";
  import { UNFOLD_STEP, type DiffModel } from "../editor/diff/model";
  import {
    changedLineNear,
    parseFoldActionId,
    parseHunkActionId,
    splitPanes,
    unifiedPane,
    type DiffLayout,
    type DiffPane,
    type HunkHeaders,
  } from "../editor/diff/view";
  import type { SyntaxHighlighter } from "../editor/syntax/highlighter";
  import { editorFace } from "./editorFace.svelte";
  import { editorSettings } from "./editorSettings";
  import EditorSurface from "./EditorSurface.svelte";
  import ViStatusLine from "./ViStatusLine.svelte";
  import type { ViStatus } from "./viSurface";
  import { highlightFor } from "./highlighting";
  import { ScrollLink } from "./scrollLink";
  import { NO_EFFECTS, surfaceSettings } from "./surfaceSettings";

  interface Props {
    model: DiffModel;
    layout: DiffLayout;
    /** Bump when the model changed behind this component's back (whole file on/off). */
    revision?: number;
    /** Both sides' whole texts, for their colours (H2); `null` where a side has none. */
    oldText: string | null;
    newText: string | null;
    /** For the language of both sides. */
    fileName: string;
    /** ⏎ on a line: open the changed file at this zero-based line (H5). */
    onOpen?: (line: number) => void;
    /** Sees every key first; `true` means it was handled. */
    interceptKey?: (e: KeyboardEvent) => boolean;
    /** A header row above each hunk, with a discard button (H6); none for "Compare". */
    hunkHeaders?: HunkHeaders | null;
    /** "Discard" on a hunk's header, or ⌘⌫ inside it (H9). */
    onDiscardHunk?: (hunk: number) => void;
  }

  let {
    model,
    layout,
    revision = 0,
    oldText,
    newText,
    fileName,
    onOpen,
    interceptKey,
    hunkHeaders = null,
    onDiscardHunk,
  }: Props = $props();

  const face = editorFace();
  const settings = $derived({
    ...surfaceSettings($editorSettings, false),
    fontFamily: face.family,
    fontWeight: face.weight,
    // A diff wants the text and the change colours, nothing on top of them.
    effects: { ...NO_EFFECTS, bracketColors: $editorSettings.bracketColors },
  });

  /** Bumped when a fold opens here; with `revision`, what the panes are rebuilt from. */
  let unfolds = $state(0);
  /** An injected grammar arrived: colours changed. */
  let coloured = $state(0);

  const panes = $derived.by((): DiffPane[] => {
    void revision;
    void unfolds;
    if (layout === "unified") return [unifiedPane(model.unified(), UNFOLD_STEP, hunkHeaders)];
    const { left, right } = splitPanes(model.split(), UNFOLD_STEP, hunkHeaders);
    return [left, right];
  });

  /** A read-only document per pane. The extra line break keeps a trailing empty row (see `bodyForStore`). */
  const docs = $derived(
    panes.map((p) => new EditorDocument(`${p.text}\n`, { indentFallback: { kind: "spaces", size: 4 } })),
  );

  /** What is folded, for every pane at once; new with the panes. */
  const folds = $derived.by(() => {
    void docs;
    return new FoldState(null);
  });
  /** Bumped when one pane changed the folds: the other redraws. */
  let folded = $state(0);

  // Each side's whole text, highlighted on its own (H2) — tagged with the texts
  // they were made for: until the next pair is ready, a new file must not be
  // coloured (or worse, queried past its end) by the previous file's parsers.
  interface Sides {
    old: SyntaxHighlighter | null;
    new: SyntaxHighlighter | null;
    texts: { old: string | null; new: string | null };
  }
  const NO_SIDES: Sides = { old: null, new: null, texts: { old: null, new: null } };
  let sides = $state.raw<Sides>(NO_SIDES);
  let sideGeneration = 0;

  $effect(() => {
    const texts = { old: oldText, new: newText };
    const name = fileName;
    const mine = ++sideGeneration;
    void attachSides(texts, name, mine);
    return () => {
      sideGeneration++;
      disposeSides();
    };
  });

  async function highlighterFor(text: string | null, name: string, stale: () => boolean) {
    if (text === null) return null;
    const doc = new EditorDocument(text, { indentFallback: { kind: "spaces", size: 4 } });
    return highlightFor(doc, { fileName: name, onColours: () => coloured++, stale });
  }

  async function attachSides(texts: { old: string | null; new: string | null }, name: string, mine: number) {
    const stale = () => mine !== sideGeneration;
    const [old, now] = await Promise.all([
      highlighterFor(texts.old, name, stale),
      highlighterFor(texts.new, name, stale),
    ]);
    if (!stale()) sides = { old, new: now, texts };
  }

  function disposeSides(): void {
    sides.old?.dispose();
    sides.new?.dispose();
    sides = NO_SIDES;
  }

  const highlights = $derived.by(() => {
    const current = sides.texts.old === oldText && sides.texts.new === newText ? sides : NO_SIDES;
    return panes.map((p) => new DiffHighlight(p.sources, current));
  });

  // ---------------------------------------------------------------- surfaces

  let surfaces = $state<(EditorSurface | null)[]>([null, null]);
  /** The pane the cursor was last in — where ⌥↓ and ⏎ act. */
  let active = $state(0);
  /** Each pane's Vi status (V8), shown in a bar under the active one while Vi is on. */
  let viStatuses = $state<(ViStatus | null)[]>([null, null]);
  const scrollLink = new ScrollLink();

  function onScrollPos(index: number, top: number, left: number): void {
    if (panes.length < 2 || !scrollLink.scrolled(String(index))) return;
    surfaces[1 - index]?.scrollToPos(top, left);
  }

  function onLineAction(action: string): void {
    const hunk = parseHunkActionId(action);
    if (hunk) {
      onDiscardHunk?.(hunk.hunk);
      return;
    }
    const fold = parseFoldActionId(action);
    if (!fold) return;
    model.unfold(fold.gap, fold.action);
    unfolds++;
  }

  /** The cursor's line in the active pane. */
  function cursorLine(): number {
    return docs[active]?.selection.head.line ?? 0;
  }

  /** Moves to the next (`1`) or previous (`-1`) hunk from the cursor (H9). */
  export function goToHunk(step: 1 | -1): void {
    const starts = panes[active]?.hunkStarts ?? [];
    const line = cursorLine();
    const target = step === 1 ? starts.find((s) => s > line) : [...starts].reverse().find((s) => s < line);
    if (target === undefined) return;
    surfaces[active]?.goToLine(target);
    // The other side of a split view keeps its cursor on the same row.
    if (panes.length === 2) docs[1 - active]?.setSelection({ anchor: pos(target, 0), head: pos(target, 0) });
  }

  /** The changed file's line under the cursor, if the diff has a changed side. */
  export function openLine(): number | null {
    const pane = panes[active];
    return pane ? changedLineNear(pane, cursorLine()) : null;
  }

  /** Vi in a diff (V1): `]c`/`[c` go from hunk to hunk (H9), `gf` opens the changed file. */
  function onViEffect(effect: ViEffect): void {
    if (effect.type === "hunk") goToHunk(effect.dir);
    else if (effect.type === "openFile") {
      const line = openLine();
      if (line !== null) onOpen?.(line);
    }
  }

  function onKey(e: KeyboardEvent): boolean {
    if (e.altKey && !e.metaKey && !e.ctrlKey && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      goToHunk(e.key === "ArrowDown" ? 1 : -1);
      return true;
    }
    if (e.key === "Backspace" && e.metaKey && !e.altKey && !e.ctrlKey && !e.shiftKey) {
      const hunk = panes[active]?.hunkOf[cursorLine()];
      if (hunk !== null && hunk !== undefined && hunkHeaders?.discard) onDiscardHunk?.(hunk);
      return true;
    }
    if (e.key === "Enter" && !e.metaKey && !e.altKey && !e.ctrlKey && !e.shiftKey) {
      const line = openLine();
      if (line !== null) onOpen?.(line);
      return true;
    }
    return interceptKey?.(e) ?? false;
  }

  /** Focuses the active pane. */
  export function focus(): void {
    surfaces[active]?.focus();
  }

  /** Puts the cursor on the first hunk, for a freshly opened diff. */
  export function showFirstChange(): void {
    const first = panes[0]?.hunkStarts[0];
    if (first !== undefined) surfaces[active]?.goToLine(first);
  }
</script>

<div class="diff-panes" class:split={panes.length === 2}>
  {#each panes as pane, index (index)}
    <div class="pane" onfocusin={() => (active = index)}>
      <EditorSurface
        bind:this={surfaces[index]}
        doc={docs[index]}
        {settings}
        {fileName}
        readOnly
        revision={coloured + folded}
        {folds}
        onFolds={() => folded++}
        decorations={pane.decorations}
        highlighter={highlights[index]}
        onLineAction={(_line, action) => onLineAction(action)}
        onScrollPos={(top, left) => onScrollPos(index, top, left)}
        interceptKey={onKey}
        onViEffect={onViEffect}
        onViStatus={(s) => (viStatuses[index] = s)}
      />
      {#if active === index && viStatuses[index]}
        <div class="vi-bar"><ViStatusLine status={viStatuses[index]!} /></div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .diff-panes {
    display: grid;
    grid-template-columns: 1fr;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
  }

  .diff-panes.split {
    grid-template-columns: 1fr 1fr;
    gap: 1px;
    background: var(--ax-border);
  }

  .pane {
    position: relative;
    min-width: 0;
    min-height: 0;
  }

  /* No footer of its own here: Vi's status floats over the pane's bottom edge. */
  .vi-bar {
    position: absolute;
    left: var(--ax-space-3);
    right: var(--ax-space-3);
    bottom: var(--ax-space-2);
    display: flex;
    padding: var(--ax-space-1) var(--ax-space-3);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    pointer-events: none;
  }
</style>
