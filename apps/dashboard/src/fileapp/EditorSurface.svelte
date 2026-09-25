<!--
  The editor surface (`docs/plans/editor.md`, ED1.2): draws an `EditorDocument`
  and turns keys, typing and the mouse into model commands.

  * **Only visible rows exist in the DOM** (plus a small overscan). A sizer as tall
    as every visual row gives the scroller its range; rows, selection runs and the
    cursor are absolutely placed at `row × row height`.
  * **All geometry is arithmetic.** Every bundled font is monospace, so one cell
    width is measured and `editor/geometry.ts` does the rest — no per-character
    DOM measuring, and every rule about where things go is unit-tested there.
  * **Input goes through a hidden textarea that follows the cursor** (F2). Typing,
    dead keys and the Mac input method arrive as `input`/`composition` events;
    during a composition the textarea becomes visible in place, so marked text
    shows where it will land. Keys the editor owns are handled on `keydown`.
  * **The clipboard is the native one.** ⌘C/⌘X put the text into the textarea and
    select it before the browser copies, and `copy`/`cut`/`paste` events carry the
    data — no clipboard permission needed. A line copied without a selection is
    remembered, so pasting exactly that text inserts it as a line (F5).
  * **Decorations are the owner's** (`docs/plans/git-layer.md`, H3): with a
    `decorations` prop the gutter shows their labels instead of line numbers, and
    each line can carry a colour, word marks, and a label with buttons on an
    empty line — what the diff view (and later the Git-Gutter) is drawn with.
  * **`tick` is the one redraw signal.** `EditorDocument` is a plain mutable
    class, not Svelte state, so every `doc.*` call in this file is followed by
    `changed()` (which bumps `tick`); a change the owner makes behind the
    surface's back arrives through the `revision` prop instead.
-->
<script lang="ts">
  import { onMount, tick as nextTick, untrack } from "svelte";

  import { commentPrefixFor, copyText, cut, paste, run, type ClipboardText, type Command } from "../editor/commands";
  import { indentGuides, type LineDecoration, type LineDecorations } from "../editor/decorations";
  import type { EditorDocument } from "../editor/document";
  import { cursorCell, posAtCell, rowSlice, selectionRuns } from "../editor/geometry";
  import { gutterDigits, lineLabel } from "../editor/gutter";
  import { keyAction, type Effect } from "../editor/keymap";
  import { pos, selectionRange, type Pos } from "../editor/position";
  import { wordAt } from "../editor/text";
  import { rowSegments, type Span } from "../editor/syntax/paint";
  import { VisualLayout } from "../editor/visual";
  import { CursorGlide } from "./cursorGlide";
  import { KEEP_SCROLL } from "./keepScroll";
  import type { SurfaceSettings } from "./surfaceSettings";

  interface Props {
    doc: EditorDocument;
    settings: SurfaceSettings;
    /** File name, for the comment prefix of ⌘/. */
    fileName?: string;
    readOnly?: boolean;
    autofocus?: boolean;
    /** ⌘S, ⌘O, ⌥Z — what the surface cannot do itself. */
    onEffect?: (effect: Effect) => void;
    /** After anything changed the text or the selection. */
    onChange?: () => void;
    /**
     * Bump when the owner changed the document behind the surface's back (a
     * reload, restoring kept work): the surface only notices its own edits.
     */
    revision?: number;
    /**
     * Colours for the lines on screen (ED2, G4) — a `SyntaxHighlighter`, or
     * anything else that can answer the same question. Without one, plain text.
     */
    highlighter?: { spans(first: number, last: number, options?: { brackets?: boolean }): Map<number, Span[]> } | null;
    /** The first logical line on screen, after every scroll (the preview follows it, G8). */
    onTopLine?: (line: number) => void;
    /** Pixel scroll position after every scroll — for a second surface kept in step (H12). */
    onScrollPos?: (top: number, left: number) => void;
    /** Per-line colours, gutter labels, marks and buttons (H3); replaces the line numbers. */
    decorations?: LineDecorations | null;
    /** A button of a line's decoration was clicked. */
    onLineAction?: (line: number, action: string) => void;
    /** Sees every key first; returning `true` means it was handled and the surface ignores it. */
    interceptKey?: (e: KeyboardEvent) => boolean;
  }

  let {
    doc,
    settings,
    fileName = "",
    readOnly = false,
    autofocus = false,
    onEffect,
    onChange,
    revision = 0,
    highlighter = null,
    onTopLine,
    onScrollPos,
    decorations = null,
    onLineAction,
    interceptKey,
  }: Props = $props();

  /** Rows drawn above and below the viewport, so fast scrolling shows no gaps. */
  const OVERSCAN = 8;
  /** Space between the line numbers and the text, in cells. */
  const TEXT_GAP_CELLS = 1;
  /** A jump further than this many rows scrolls smoothly, if that is on. */
  const SMOOTH_SCROLL_ROWS = 3;
  /** Rows kept above a line jumped to with `goToLine`. */
  const GO_TO_MARGIN_ROWS = 3;
  /** Commands that only move the selection — the ones a read-only surface allows. */
  const NON_EDITING = new Set<Command["type"]>(["move", "selectAll", "selectLine"]);
  const NO_DECORATION: LineDecoration = {};

  let scroller: HTMLDivElement;
  let measurer: HTMLSpanElement;
  let input: HTMLTextAreaElement;

  /** Bumped after every model change; everything drawn derives from it. */
  let tick = $state(0);
  let charW = $state(8);
  let scrollTop = $state(0);
  let viewW = $state(800);
  let viewH = $state(600);
  let composing = $state(false);
  let focused = $state(false);
  let lastClip: ClipboardText | null = null;

  const rowH = $derived(Math.round(settings.fontSize * settings.lineHeight));
  const gutterCells = $derived.by(() => {
    if (decorations) return decorations.gutterCells;
    void tick; // the line count changes with edits
    return gutterDigits(doc.store.lineCount(), settings.lineNumbers) + 2;
  });
  const gutterW = $derived(gutterCells * charW);
  /** Where the text starts, relative to the scroller's left edge. */
  const textLeft = $derived(gutterW + TEXT_GAP_CELLS * charW);
  const wrapCells = $derived(Math.max(8, Math.floor((viewW - textLeft - 2 * charW) / charW)));

  // macOS "Reduce motion" turns the glide and smooth scrolling off (G7).
  const motionQuery = typeof matchMedia === "function" ? matchMedia("(prefers-reduced-motion: reduce)") : null;
  let reducedMotion = $state(motionQuery?.matches ?? false);
  const fx = $derived(
    reducedMotion ? { ...settings.effects, cursor: "off" as const, smoothScroll: false } : settings.effects,
  );

  // One layout per document (its wrap cache is worth keeping); the options
  // follow the settings and the viewport in the effect below.
  let layout = $derived(
    new VisualLayout(
      doc.store,
      untrack(() => ({ wrap: settings.wrap, width: 80, tabSize: settings.tabSize })),
    ),
  );
  $effect(() => {
    layout.setOptions({ wrap: settings.wrap, width: wrapCells, tabSize: settings.tabSize });
    // `untrack`: reading `tick` here would make this effect re-run itself.
    untrack(() => tick++);
  });

  // The owner changed the document: rewrap and redraw.
  $effect(() => {
    void revision;
    untrack(() => {
      layout.refresh();
      tick++;
    });
  });

  const ctx = $derived({
    tabSize: settings.tabSize,
    layout,
    pageRows: Math.max(1, Math.floor(viewH / rowH) - 1),
    commentPrefix: commentPrefixFor(fileName),
  });

  const view = $derived.by(() => {
    void tick;
    const total = layout.totalRows;
    const first = Math.max(0, Math.floor(scrollTop / rowH) - OVERSCAN);
    const last = Math.min(total - 1, Math.ceil((scrollTop + viewH) / rowH) + OVERSCAN);
    const slices = [];
    for (let row = first; row <= last; row++) slices.push({ row, ...rowSlice(layout, doc.store, row) });
    const firstLine = slices[0]?.line ?? 0;
    const lastLine = slices[slices.length - 1]?.line ?? 0;
    const spans =
      highlighter && slices.length > 0
        ? highlighter.spans(firstLine, lastLine, { brackets: fx.bracketColors })
        : new Map<number, Span[]>();
    const guides = fx.indentGuides
      ? indentGuides(doc.store, firstLine, lastLine, doc.indent.size, settings.tabSize)
      : new Map<number, number[]>();
    const rows = slices.map((r) => ({
      ...r,
      segments: rowSegments(doc.store.line(r.line), spans.get(r.line), r.start, r.end),
      guides: guides.get(r.line) ?? [],
      deco: decorations?.line(r.line) ?? NO_DECORATION,
    }));
    const sel = doc.selection;
    const runs = selectionRuns(layout, doc.store, selectionRange(sel), first, last, settings.tabSize);
    const marks = markRuns(rows, first, last);
    const caret = cursorCell(layout, doc.store, sel.head, settings.tabSize);
    let widest = 0;
    if (!settings.wrap) for (const r of rows) widest = Math.max(widest, r.text.length);
    const current = { top: layout.firstRow(sel.head.line), rows: layout.rowStarts(sel.head.line).length };
    return { total, rows, runs, marks, caret, cursorLine: sel.head.line, current, widest };
  });

  /** Where the word marks of the lines on screen fall, as runs per visual row. */
  function markRuns(rows: { line: number; sub: number; deco: LineDecoration }[], first: number, last: number) {
    const out: { key: string; row: number; from: number; to: number; kind: string }[] = [];
    for (const r of rows) {
      if (r.sub !== 0 || !r.deco.marks) continue;
      for (const m of r.deco.marks) {
        const markRange = { start: pos(r.line, m.from), end: pos(r.line, m.to) };
        for (const run of selectionRuns(layout, doc.store, markRange, first, last, settings.tabSize)) {
          out.push({ key: `${run.row}:${run.from}:${m.kind}`, row: run.row, from: run.from, to: run.to, kind: m.kind });
        }
      }
    }
    return out;
  }

  // ---------------------------------------------------------------- model glue

  /** After a command: redraw, keep the cursor in view, tell the owner. */
  function changed(): void {
    layout.refresh();
    tick++;
    void nextTick().then(revealCursor);
    onChange?.();
  }

  /** `ctx`, stamped with the moment the command runs (undo grouping needs it). */
  function ctxNow(): typeof ctx & { now: number } {
    return { ...ctx, now: Date.now() };
  }

  function exec(cmd: Command): void {
    if (readOnly && !NON_EDITING.has(cmd.type)) return;
    run(doc, cmd, ctxNow());
    changed();
  }

  function revealCursor(): void {
    if (!scroller) return;
    const { row, cell } = cursorCell(layout, doc.store, doc.selection.head, settings.tabSize);
    const y = row * rowH;
    let top = scroller.scrollTop;
    if (y < top) top = y;
    else if (y + rowH > top + viewH) top = y + rowH - viewH;
    let left = scroller.scrollLeft;
    if (!settings.wrap) {
      const x = cell * charW;
      const room = viewW - textLeft - 4 * charW;
      if (x < left) left = Math.max(0, x - 4 * charW);
      else if (x > left + room) left = x - room;
    }
    if (top === scroller.scrollTop && left === scroller.scrollLeft) return;
    const far = Math.abs(top - scroller.scrollTop) > SMOOTH_SCROLL_ROWS * rowH;
    scroller.scrollTo({ top, left, behavior: fx.smoothScroll && far ? "smooth" : "instant" });
  }

  // ---------------------------------------------------------------- cursor glide

  let canvas: HTMLCanvasElement;
  let surfaceEl: HTMLDivElement;
  /** A glide is running: the canvas draws the cursor, the DOM caret hides. */
  let gliding = $state(false);
  const glide = new CursorGlide(
    () => (canvas && scroller ? { canvas, scroller, styles: surfaceEl, textLeft, rowH } : null),
    (on) => (gliding = on),
  );

  $effect(() => {
    const target = { x: view.caret.cell * charW, y: view.caret.row * rowH };
    const style = fx.cursor === "off" ? null : { trail: fx.cursor === "trail", glow: fx.glow };
    untrack(() => glide.moveTo(target, style));
  });

  // ---------------------------------------------------------------- keyboard

  function onKeydown(e: KeyboardEvent): void {
    if (e.isComposing || composing) return;
    if (interceptKey?.(e)) {
      e.preventDefault();
      return;
    }
    const action = keyAction({
      key: e.key,
      meta: e.metaKey,
      alt: e.altKey,
      shift: e.shiftKey,
      ctrl: e.ctrlKey,
      keyCode: e.keyCode,
    });
    if (!action) return;
    if ("command" in action) {
      e.preventDefault();
      exec(action.command);
      return;
    }
    switch (action.effect) {
      case "copy":
      case "cut":
        // Let the browser copy what the textarea holds; the event handlers
        // below also set the data, whichever the engine honours.
        if (action.effect === "cut" && readOnly) return;
        input.value = copyText(doc).text;
        input.select();
        return;
      case "paste":
        if (readOnly) e.preventDefault();
        return;
      default:
        e.preventDefault();
        onEffect?.(action.effect);
    }
  }

  function onCopy(e: ClipboardEvent): void {
    e.preventDefault();
    lastClip = copyText(doc);
    e.clipboardData?.setData("text/plain", lastClip.text);
    input.value = "";
  }

  function onCut(e: ClipboardEvent): void {
    e.preventDefault();
    if (readOnly) return;
    lastClip = cut(doc, ctxNow());
    e.clipboardData?.setData("text/plain", lastClip.text);
    input.value = "";
    changed();
  }

  function onPaste(e: ClipboardEvent): void {
    e.preventDefault();
    if (readOnly) return;
    const text = e.clipboardData?.getData("text/plain") ?? "";
    if (!text) return;
    const wholeLine = lastClip !== null && lastClip.wholeLine && lastClip.text === text;
    paste(doc, { text, wholeLine }, ctxNow());
    changed();
  }

  function onInput(e: Event): void {
    if ((e as InputEvent).isComposing) return;
    // WebKit sometimes ends a composition (a cancelled dead key, say) with a
    // plain input event and no compositionend; the flag must not stay stuck,
    // or every key the editor owns would be ignored from then on.
    composing = false;
    commitInput();
  }

  function onCompositionEnd(): void {
    composing = false;
    commitInput();
  }

  /** Inserts whatever the textarea holds and empties it. */
  function commitInput(): void {
    const text = input.value;
    input.value = "";
    if (text) exec({ type: "insert", text });
  }

  /** Leaving mid-composition drops the unfinished marked text (a lone dead key). */
  function onBlur(): void {
    focused = false;
    if (composing) {
      composing = false;
      input.value = "";
    }
  }

  // ---------------------------------------------------------------- mouse

  /** The text position under a mouse event. */
  function posAt(e: MouseEvent): Pos {
    const rect = scroller.getBoundingClientRect();
    const y = e.clientY - rect.top + scroller.scrollTop;
    const x = e.clientX - rect.left + scroller.scrollLeft - textLeft;
    return posAtCell(layout, doc.store, Math.floor(y / rowH), x / charW, settings.tabSize);
  }

  function wordRange(p: Pos): { start: Pos; end: Pos } {
    const w = wordAt(doc.store.line(p.line), p.col);
    return { start: pos(p.line, w.start), end: pos(p.line, w.end) };
  }

  function lineRange(p: Pos): { start: Pos; end: Pos } {
    const next = p.line + 1 < doc.store.lineCount() ? pos(p.line + 1, 0) : pos(p.line, doc.store.line(p.line).length);
    return { start: pos(p.line, 0), end: next };
  }

  /**
   * Click places the cursor, ⇧-click extends, a double click selects a word and a
   * triple click a line; dragging then extends by the same unit it started with.
   */
  function onMousedown(e: MouseEvent): void {
    if (e.button !== 0) return;
    e.preventDefault();
    input.focus();
    const unit = e.detail >= 3 ? lineRange : e.detail === 2 ? wordRange : null;
    const start = posAt(e);
    const origin = unit ? unit(start) : { start, end: start };
    if (e.shiftKey && !unit) doc.setSelection({ anchor: doc.selection.anchor, head: start });
    else doc.setSelection({ anchor: origin.start, head: origin.end });
    changed();

    const onMove = (ev: MouseEvent) => {
      const at = posAt(ev);
      const hit = unit ? unit(at) : { start: at, end: at };
      const forward = at.line > origin.start.line || (at.line === origin.start.line && at.col >= origin.start.col);
      doc.setSelection(forward ? { anchor: origin.start, head: hit.end } : { anchor: origin.end, head: hit.start });
      changed();
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  /** A click in the gutter selects the whole line. */
  function onGutterMousedown(e: MouseEvent): void {
    e.preventDefault();
    e.stopPropagation();
    input.focus();
    const r = lineRange(posAt(e));
    doc.setSelection(e.shiftKey ? { anchor: doc.selection.anchor, head: r.end } : { anchor: r.start, head: r.end });
    changed();
  }

  // ---------------------------------------------------------------- measuring

  function measure(): void {
    if (!measurer) return;
    const w = measurer.getBoundingClientRect().width / 64;
    if (w > 0) charW = w;
  }

  // Re-measure whenever the font changes, once the face has loaded.
  $effect(() => {
    const font = `${settings.fontWeight} ${settings.fontSize}px "${settings.fontFamily}"`;
    void document.fonts
      .load(font)
      .catch(() => [])
      .then(() => {
        measure();
        tick++;
      });
  });

  onMount(() => {
    measure();
    const observer = new ResizeObserver(() => {
      viewW = scroller.clientWidth;
      viewH = scroller.clientHeight;
    });
    observer.observe(scroller);
    if (autofocus) input.focus();
    const onMotion = (e: MediaQueryListEvent) => (reducedMotion = e.matches);
    motionQuery?.addEventListener("change", onMotion);
    return () => {
      observer.disconnect();
      motionQuery?.removeEventListener("change", onMotion);
      glide.dispose();
    };
  });

  /** Focuses the surface (the owner calls this after opening a file). */
  export function focus(): void {
    input?.focus();
  }

  /** Scrolls so `line` is the first on screen (the preview leading, G8). */
  export function scrollToLine(line: number): void {
    if (scroller) scroller.scrollTop = layout.firstRow(Math.min(line, doc.store.lineCount() - 1)) * rowH;
  }

  /** Scrolls to a pixel position (a second surface following this one, H12). */
  export function scrollToPos(top: number, left: number): void {
    if (!scroller) return;
    scroller.scrollTop = top;
    scroller.scrollLeft = left;
  }

  /**
   * Puts the cursor at the start of `line` and scrolls it into view, a few rows
   * below the top edge so what leads up to it stays visible (H9: next change).
   */
  export function goToLine(line: number): void {
    const target = Math.max(0, Math.min(line, doc.store.lineCount() - 1));
    doc.setSelection({ anchor: pos(target, 0), head: pos(target, 0) });
    layout.refresh();
    tick++;
    onChange?.();
    // After the next render: a freshly opened file's sizer is not yet as tall
    // as the text, and a scroll past its end would be cut short to 0.
    void nextTick().then(() => {
      if (!scroller) return;
      const top = Math.max(0, (layout.firstRow(target) - GO_TO_MARGIN_ROWS) * rowH);
      const far = Math.abs(top - scroller.scrollTop) > SMOOTH_SCROLL_ROWS * rowH;
      scroller.scrollTo({ top, behavior: fx.smoothScroll && far ? "smooth" : "instant" });
    });
  }
</script>

<div
  class="surface"
  class:focused
  class:glow={fx.glow}
  bind:this={surfaceEl}
  style:--cell="{charW}px"
  style:--row="{rowH}px"
  style:font-family={`"${settings.fontFamily}", var(--ax-font-mono)`}
  style:font-weight={settings.fontWeight}
  style:font-size="{settings.fontSize}px"
  style:font-variant-ligatures={settings.ligatures ? "normal" : "none"}
  style:tab-size={settings.tabSize}
>
  <span class="measure" bind:this={measurer} aria-hidden="true">{"0".repeat(64)}</span>
  <!-- `KEEP_SCROLL`: an IDE dock moves panes around in the DOM, which resets
       this position without a scroll event (`fileapp/keepScroll.ts`). -->
  <div
    class="scroller"
    {...KEEP_SCROLL}
    bind:this={scroller}
    onscroll={() => {
      scrollTop = scroller.scrollTop;
      onTopLine?.(layout.lineAt(Math.floor(scrollTop / rowH)).line);
      onScrollPos?.(scroller.scrollTop, scroller.scrollLeft);
    }}
    onmousedown={onMousedown}
    role="presentation"
  >
    <div
      class="sizer"
      style:height="{view.total * rowH}px"
      style:width={settings.wrap ? "100%" : `${textLeft + (view.widest + 8) * charW}px`}
    >
      <div class="gutter" style:width="{gutterW}px" onmousedown={onGutterMousedown} role="presentation">
        {#each view.rows as r (r.row)}
          {#if decorations}
            <div class="number decorated ln-{r.deco.kind ?? 'none'}" style:top="{r.row * rowH}px">
              {r.sub === 0 ? (r.deco.gutter ?? "") : ""}
            </div>
          {:else if r.sub === 0}
            <div class="number" class:current={r.line === view.cursorLine} style:top="{r.row * rowH}px">
              {lineLabel(r.line, view.cursorLine, settings.lineNumbers)}
            </div>
          {/if}
        {/each}
      </div>
      <div class="content" style:left="{textLeft}px">
        {#each view.rows as r (r.row)}
          {#if r.deco.kind}
            <div class="line-bg ln-{r.deco.kind}" style:top="{r.row * rowH}px"></div>
          {/if}
        {/each}
        {#each view.marks as m (m.key)}
          <div
            class="mark mk-{m.kind}"
            style:top="{m.row * rowH}px"
            style:left="{m.from * charW}px"
            style:width="{(m.to - m.from) * charW}px"
          ></div>
        {/each}
        {#if fx.currentLine}
          <div
            class="current-line"
            style:top="{view.current.top * rowH}px"
            style:height="{view.current.rows * rowH}px"
          ></div>
        {/if}
        {#each view.rows as r (r.row)}
          {#each r.guides as cell (cell)}
            <div class="guide" style:top="{r.row * rowH}px" style:left="{cell * charW}px"></div>
          {/each}
        {/each}
        {#each view.runs as run (run.row)}
          <div
            class="selection"
            style:top="{run.row * rowH}px"
            style:left="{run.from * charW}px"
            style:width="{(run.to - run.from) * charW}px"
          ></div>
        {/each}
        {#each view.rows as r (r.row)}
          <!-- Line breaks only inside tags: whitespace between the segments would show. -->
          <div
            class="row"
            style:top="{r.row * rowH}px"
            style:padding-left="{r.indent * charW}px"
          >{#each r.segments as seg, i (i)}{#if seg.token}<span class="tk-{seg.token}">{seg.text}</span
              >{:else}{seg.text}{/if}{/each}</div
          >
          {#if r.sub === 0 && (r.deco.label || r.deco.actions)}
            <!-- A fold or a note: the label, and buttons the mouse can reach. -->
            <div class="line-label" style:top="{r.row * rowH}px">
              {#if r.deco.label}<span class="label-text">{r.deco.label}</span>{/if}
              {#each r.deco.actions ?? [] as action (action.id)}
                <button
                  type="button"
                  class="line-action"
                  title={action.title}
                  onmousedown={(e) => e.stopPropagation()}
                  onclick={() => onLineAction?.(r.line, action.id)}>{action.label}</button
                >
              {/each}
            </div>
          {/if}
        {/each}
        {#key tick}
          <div
            class="caret"
            class:hidden={!focused || composing || gliding}
            style:top="{view.caret.row * rowH}px"
            style:left="{view.caret.cell * charW}px"
          ></div>
        {/key}
        <textarea
          class="input"
          class:composing
          bind:this={input}
          style:top="{view.caret.row * rowH}px"
          style:left="{view.caret.cell * charW}px"
          spellcheck="false"
          autocapitalize="off"
          autocomplete="off"
          aria-label="Editor"
          readonly={readOnly}
          onkeydown={onKeydown}
          oninput={onInput}
          oncompositionstart={() => (composing = true)}
          oncompositionend={onCompositionEnd}
          oncopy={onCopy}
          oncut={onCut}
          onpaste={onPaste}
          onfocus={() => (focused = true)}
          onblur={onBlur}
        ></textarea>
      </div>
    </div>
  </div>
  <canvas class="glide" class:on={gliding} bind:this={canvas} aria-hidden="true"></canvas>
</div>

<style>
  .surface {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    color: var(--ax-text);
    background: var(--ax-surface-1);
    line-height: var(--row);
  }

  .measure {
    position: absolute;
    visibility: hidden;
    white-space: pre;
    pointer-events: none;
  }

  .scroller {
    position: absolute;
    inset: 0;
    overflow: auto;
    cursor: text;
  }

  .sizer {
    position: relative;
    min-height: 100%;
    /* At least the viewport: the current-line tint reaches the right edge. */
    min-width: 100%;
  }

  .gutter {
    position: sticky;
    left: 0;
    top: 0;
    height: 100%;
    float: left;
    z-index: 2;
    background: var(--ax-surface-1);
    cursor: default;
  }

  .number {
    position: absolute;
    right: var(--cell);
    height: var(--row);
    color: var(--ax-text-muted);
    opacity: 0.55;
    text-align: right;
    font-variant-numeric: tabular-nums;
    user-select: none;
  }

  .number.current {
    color: var(--ax-accent);
    opacity: 1;
  }

  .content {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
  }

  /* Syntax colours (G5): one --ax-syntax-* token per class; every theme sets them.
     Colour only — no bold or italic, which would make the browser fake a face
     that is not loaded (F13). */
  .tk-keyword { color: var(--ax-syntax-keyword); }
  .tk-string { color: var(--ax-syntax-string); }
  .tk-number { color: var(--ax-syntax-number); }
  .tk-comment { color: var(--ax-syntax-comment); }
  .tk-function { color: var(--ax-syntax-function); }
  .tk-type { color: var(--ax-syntax-type); }
  .tk-variable { color: var(--ax-syntax-variable); }
  .tk-constant { color: var(--ax-syntax-constant); }
  .tk-property { color: var(--ax-syntax-property); }
  .tk-operator { color: var(--ax-syntax-operator); }
  .tk-punctuation { color: var(--ax-syntax-punctuation); }
  .tk-tag { color: var(--ax-syntax-tag); }
  .tk-attribute { color: var(--ax-syntax-attribute); }
  .tk-heading { color: var(--ax-syntax-heading); }
  .tk-link { color: var(--ax-syntax-link); text-decoration: underline; }
  .tk-emphasis { color: var(--ax-syntax-emphasis); }
  .tk-code { color: var(--ax-syntax-code); }
  .tk-bracket-1 { color: var(--ax-editor-bracket-1); }
  .tk-bracket-2 { color: var(--ax-editor-bracket-2); }
  .tk-bracket-3 { color: var(--ax-editor-bracket-3); }

  /* Line decorations (H3): the diff's colours. The gutter keeps its colour per
     kind so a change is seen at the edge even when scrolled sideways. */
  .line-bg {
    position: absolute;
    left: calc(-1 * var(--cell));
    right: 0;
    height: var(--row);
    pointer-events: none;
  }

  .ln-add {
    background: var(--ax-diff-add);
  }

  .ln-remove {
    background: var(--ax-diff-remove);
  }

  .ln-fold,
  .ln-note,
  .ln-hunk {
    background: var(--ax-diff-hunk);
  }

  .ln-blank {
    background: var(--ax-surface-2);
  }

  .number.decorated {
    left: 0;
    right: 0;
    padding-right: var(--cell);
    opacity: 0.8;
    white-space: pre;
  }

  .number.ln-add {
    color: var(--ax-success);
  }

  .number.ln-remove {
    color: var(--ax-danger);
  }

  .number.ln-fold {
    text-align: center;
    padding-right: 0;
  }

  .mark {
    position: absolute;
    height: var(--row);
    pointer-events: none;
    border-radius: var(--ax-radius-sm);
  }

  .mk-add-word {
    background: var(--ax-diff-add-word);
  }

  .mk-remove-word {
    background: var(--ax-diff-remove-word);
  }

  .line-label {
    position: absolute;
    left: 0;
    height: var(--row);
    display: flex;
    align-items: center;
    gap: var(--cell);
    color: var(--ax-text-muted);
    white-space: pre;
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
  }

  .line-action {
    padding: 0 var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    color: var(--ax-text);
    font: inherit;
    line-height: 1.4;
    cursor: pointer;
  }

  .line-action:hover {
    border-color: var(--ax-accent);
    color: var(--ax-accent);
  }

  .current-line {
    position: absolute;
    left: calc(-1 * var(--cell));
    right: 0;
    background: var(--ax-editor-current-line);
    pointer-events: none;
  }

  .guide {
    position: absolute;
    width: 1px;
    height: var(--row);
    background: var(--ax-editor-indent-guide);
    pointer-events: none;
  }

  .glide {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    z-index: 4;
    pointer-events: none;
    visibility: hidden;
  }

  .glide.on {
    visibility: visible;
  }

  .glow .caret {
    box-shadow: 0 0 8px 1px var(--ax-editor-glow);
  }

  .glow .number.current {
    text-shadow: 0 0 8px var(--ax-editor-glow);
  }

  .row {
    position: absolute;
    left: 0;
    height: var(--row);
    white-space: pre;
    pointer-events: none;
  }

  .selection {
    position: absolute;
    height: var(--row);
    background: var(--ax-accent-muted);
    pointer-events: none;
  }

  .surface:not(.focused) .selection {
    opacity: 0.6;
  }

  .caret {
    position: absolute;
    width: 2px;
    height: var(--row);
    background: var(--ax-accent);
    pointer-events: none;
    animation: blink 1.1s steps(1) 0.5s infinite;
  }

  .caret.hidden {
    display: none;
  }

  @keyframes blink {
    50% {
      opacity: 0;
    }
  }

  .input {
    position: absolute;
    width: 1px;
    height: var(--row);
    padding: 0;
    border: 0;
    margin: 0;
    resize: none;
    overflow: hidden;
    outline: none;
    background: transparent;
    color: transparent;
    caret-color: transparent;
    font: inherit;
    line-height: var(--row);
    white-space: pre;
    z-index: 3;
  }

  /* While the input method composes, its marked text shows right where it will land. */
  .input.composing {
    width: auto;
    min-width: 1em;
    color: var(--ax-text);
    background: var(--ax-surface-2);
    text-decoration: underline;
  }
</style>
