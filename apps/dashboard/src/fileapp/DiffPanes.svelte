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
-->
<script lang="ts">
  import { EditorDocument } from "../editor/document";
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
  import { detectLanguage } from "../editor/syntax/languages";
  import { SyntaxHighlighter } from "../editor/syntax/highlighter";
  import { editorFace } from "./editorFace.svelte";
  import { editorSettings } from "./editorSettings";
  import EditorSurface from "./EditorSurface.svelte";
  import { grammarRuntime } from "./grammars";
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

  async function highlighterFor(text: string | null, name: string): Promise<SyntaxHighlighter | null> {
    if (text === null) return null;
    const id = detectLanguage(name, text.split("\n", 1)[0]);
    if (!id) return null;
    const doc = new EditorDocument(text, { indentFallback: { kind: "spaces", size: 4 } });
    return SyntaxHighlighter.create(doc, grammarRuntime, id, () => coloured++);
  }

  async function attachSides(texts: { old: string | null; new: string | null }, name: string, mine: number) {
    const [old, now] = await Promise.all([highlighterFor(texts.old, name), highlighterFor(texts.new, name)]);
    if (mine !== sideGeneration) {
      old?.dispose();
      now?.dispose();
      return;
    }
    sides = { old, new: now, texts };
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
        revision={coloured}
        decorations={pane.decorations}
        highlighter={highlights[index]}
        onLineAction={(_line, action) => onLineAction(action)}
        onScrollPos={(top, left) => onScrollPos(index, top, left)}
        interceptKey={onKey}
      />
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
</style>
