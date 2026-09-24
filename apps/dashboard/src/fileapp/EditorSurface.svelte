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
  * **`tick` is the one redraw signal.** `EditorDocument` is a plain mutable
    class, not Svelte state, so every `doc.*` call in this file is followed by
    `changed()` (which bumps `tick`); a change the owner makes behind the
    surface's back arrives through the `revision` prop instead.
-->
<script lang="ts">
  import { onMount, tick as nextTick, untrack } from "svelte";

  import { commentPrefixFor, copyText, cut, paste, run, type ClipboardText, type Command } from "../editor/commands";
  import type { EditorDocument } from "../editor/document";
  import { cursorCell, posAtCell, rowSlice, selectionRuns } from "../editor/geometry";
  import { gutterDigits, lineLabel } from "../editor/gutter";
  import { keyAction, type Effect } from "../editor/keymap";
  import { pos, selectionRange, type Pos } from "../editor/position";
  import { wordAt } from "../editor/text";
  import { VisualLayout } from "../editor/visual";
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
  }: Props = $props();

  /** Rows drawn above and below the viewport, so fast scrolling shows no gaps. */
  const OVERSCAN = 8;
  /** Commands that only move the selection — the ones a read-only surface allows. */
  const NON_EDITING = new Set<Command["type"]>(["move", "selectAll", "selectLine"]);

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
    void tick; // the line count changes with edits
    return gutterDigits(doc.store.lineCount(), settings.lineNumbers) + 2;
  });
  const gutterW = $derived(gutterCells * charW);
  const wrapCells = $derived(Math.max(8, Math.floor((viewW - gutterW - 2 * charW) / charW)));

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
    const rows = [];
    for (let row = first; row <= last; row++) rows.push({ row, ...rowSlice(layout, doc.store, row) });
    const sel = doc.selection;
    const runs = selectionRuns(layout, doc.store, selectionRange(sel), first, last, settings.tabSize);
    const caret = cursorCell(layout, doc.store, sel.head, settings.tabSize);
    let widest = 0;
    if (!settings.wrap) for (const r of rows) widest = Math.max(widest, r.text.length);
    return { total, rows, runs, caret, cursorLine: sel.head.line, widest };
  });

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
    if (y < scroller.scrollTop) scroller.scrollTop = y;
    else if (y + rowH > scroller.scrollTop + viewH) scroller.scrollTop = y + rowH - viewH;
    if (!settings.wrap) {
      const x = cell * charW;
      const room = viewW - gutterW - 4 * charW;
      if (x < scroller.scrollLeft) scroller.scrollLeft = Math.max(0, x - 4 * charW);
      else if (x > scroller.scrollLeft + room) scroller.scrollLeft = x - room;
    }
  }

  // ---------------------------------------------------------------- keyboard

  function onKeydown(e: KeyboardEvent): void {
    if (e.isComposing || composing) return;
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
    const x = e.clientX - rect.left + scroller.scrollLeft - gutterW;
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
    return () => observer.disconnect();
  });

  /** Focuses the surface (the owner calls this after opening a file). */
  export function focus(): void {
    input?.focus();
  }
</script>

<div
  class="surface"
  class:focused
  style:--cell="{charW}px"
  style:--row="{rowH}px"
  style:font-family={`"${settings.fontFamily}", var(--ax-font-mono)`}
  style:font-weight={settings.fontWeight}
  style:font-size="{settings.fontSize}px"
  style:font-variant-ligatures={settings.ligatures ? "normal" : "none"}
  style:tab-size={settings.tabSize}
>
  <span class="measure" bind:this={measurer} aria-hidden="true">{"0".repeat(64)}</span>
  <div
    class="scroller"
    bind:this={scroller}
    onscroll={() => (scrollTop = scroller.scrollTop)}
    onmousedown={onMousedown}
    role="presentation"
  >
    <div
      class="sizer"
      style:height="{view.total * rowH}px"
      style:width={settings.wrap ? "100%" : `${gutterW + (view.widest + 8) * charW}px`}
    >
      <div class="gutter" style:width="{gutterW}px" onmousedown={onGutterMousedown} role="presentation">
        {#each view.rows as r (r.row)}
          {#if r.sub === 0}
            <div class="number" class:current={r.line === view.cursorLine} style:top="{r.row * rowH}px">
              {lineLabel(r.line, view.cursorLine, settings.lineNumbers)}
            </div>
          {/if}
        {/each}
      </div>
      <div class="content" style:left="{gutterW}px">
        {#each view.runs as run (run.row)}
          <div
            class="selection"
            style:top="{run.row * rowH}px"
            style:left="{run.from * charW}px"
            style:width="{(run.to - run.from) * charW}px"
          ></div>
        {/each}
        {#each view.rows as r (r.row)}
          <div class="row" style:top="{r.row * rowH}px" style:padding-left="{r.indent * charW}px">{r.text}</div>
        {/each}
        {#key tick}
          <div
            class="caret"
            class:hidden={!focused || composing}
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
    opacity: 0.6;
    text-align: right;
    user-select: none;
  }

  .number.current {
    color: var(--ax-text);
    opacity: 1;
  }

  .content {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
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
