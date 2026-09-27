<!--
  The minimap beside the text (`docs/plans/editor.md`, ED5, T8): every visual
  row as a strip of its characters in their syntax colours, drawn on a canvas,
  with a slider for the part on screen.

  * **Geometry is `editor/minimap.ts`'s** (tested without a DOM); this only
    draws and turns the mouse into a scroll position for the owner.
  * **Only the rows the minimap shows are drawn**, once per animation frame,
    whatever changed — the text, the scroll position, the matches.
  * **Marks**: search matches (the find bar's or Vi's), the diff's added and
    removed lines (T17), and the cursor's line. Diagnostics come later.
  * **Colours are the theme's tokens**, resolved through a probe element (a
    canvas cannot read `var(--ax-*)`, and some tokens are `color-mix()`),
    and read again whenever the theme changes.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import type { TextStore } from "../editor/buffer";
  import type { LineDecorations } from "../editor/decorations";
  import { rowSlice } from "../editor/geometry";
  import { inkRuns, minimapLayout, onSlider, scrollForClick, scrollForSlider } from "../editor/minimap";
  import { rowSegments, type Span } from "../editor/syntax/paint";
  import type { VisualLayout } from "../editor/visual";

  interface Props {
    layout: VisualLayout;
    store: TextStore;
    /** Bumped on every change worth drawing again (the surface's `tick`). */
    version: number;
    rowH: number;
    viewH: number;
    scrollTop: number;
    tabSize: number;
    cursorLine: number;
    highlighter?: {
      spans(first: number, last: number, options?: { brackets?: boolean }): Map<number, Span[]>;
    } | null;
    /** Bracket pairs coloured by depth, as in the text (G7). */
    brackets?: boolean;
    /** Search matches on lines `first`–`last`: line → `[from, to)` columns. */
    matches?: (first: number, last: number) => Map<number, [number, number][]>;
    decorations?: LineDecorations | null;
    /** The minimap asks the text to scroll here. */
    onScroll: (top: number) => void;
  }

  let {
    layout,
    store,
    version,
    rowH,
    viewH,
    scrollTop,
    tabSize,
    cursorLine,
    highlighter = null,
    brackets = false,
    matches,
    decorations = null,
    onScroll,
  }: Props = $props();

  /** One visual row's height in the minimap: two pixels of ink and one of air. */
  const ROW_PX = 3;
  const INK_PX = 2;
  /** One cell's width. */
  const CELL_PX = 1;
  /** Room left of the ink, for the diff's bar. */
  const PAD_PX = 4;
  /** Width of a diff bar at the left edge. */
  const BAR_PX = 2;
  const INK_ALPHA = 0.8;

  /** Token → the custom property its colour comes from. */
  const TOKEN_VARS = [
    "keyword",
    "string",
    "number",
    "comment",
    "function",
    "type",
    "variable",
    "constant",
    "property",
    "operator",
    "punctuation",
    "tag",
    "attribute",
    "heading",
    "link",
    "emphasis",
    "code",
  ];
  const OTHER_VARS = {
    text: "--ax-text",
    slider: "--ax-minimap-slider",
    sliderActive: "--ax-minimap-slider-active",
    match: "--ax-search-current",
    cursor: "--ax-accent-muted",
    add: "--ax-diff-add",
    remove: "--ax-diff-remove",
    addBar: "--ax-success",
    removeBar: "--ax-danger",
  };

  let canvas: HTMLCanvasElement;
  let probe: HTMLSpanElement;
  let height = $state(0);
  let width = $state(0);
  let dragging = $state(false);
  let hovering = $state(false);
  let palette: Map<string, string> | null = null;
  let frame = 0;

  const input = $derived({ totalRows: layout.totalRows, rowH, viewH, scrollTop, rowPx: ROW_PX, height });

  /** The theme's colours, resolved to something a canvas takes. */
  function colours(): Map<string, string> {
    if (palette) return palette;
    const out = new Map<string, string>();
    const resolve = (name: string) => {
      probe.style.color = `var(${name})`;
      return getComputedStyle(probe).color;
    };
    for (const t of TOKEN_VARS) out.set(t, resolve(`--ax-syntax-${t}`));
    for (let i = 1; i <= 3; i++) out.set(`bracket-${i}`, resolve(`--ax-editor-bracket-${i}`));
    for (const [key, name] of Object.entries(OTHER_VARS)) out.set(key, resolve(name));
    palette = out;
    return out;
  }

  function draw(): void {
    frame = 0;
    const ctx = canvas?.getContext("2d");
    if (!ctx || height <= 0 || width <= 0) return;
    const dpr = window.devicePixelRatio || 1;
    if (canvas.width !== Math.round(width * dpr) || canvas.height !== Math.round(height * dpr)) {
      canvas.width = Math.round(width * dpr);
      canvas.height = Math.round(height * dpr);
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    const c = colours();
    const geo = minimapLayout(input);
    if (layout.totalRows === 0) return;
    const firstLine = layout.lineAt(geo.firstRow).line;
    const lastLine = layout.lineAt(geo.lastRow).line;
    const spans = highlighter?.spans(firstLine, lastLine, { brackets }) ?? new Map<number, Span[]>();
    const found = matches?.(firstLine, lastLine) ?? new Map<number, [number, number][]>();
    for (let row = geo.firstRow; row <= geo.lastRow; row++) {
      const y = row * ROW_PX - geo.miniScroll;
      const slice = rowSlice(layout, store, row);
      const kind = decorations?.line(slice.line)?.kind;
      if (kind === "add" || kind === "remove") {
        ctx.fillStyle = c.get(kind)!;
        ctx.fillRect(0, y, width, ROW_PX);
        ctx.fillStyle = c.get(kind === "add" ? "addBar" : "removeBar")!;
        ctx.fillRect(0, y, BAR_PX, ROW_PX);
      }
      if (slice.line === cursorLine) {
        ctx.fillStyle = c.get("cursor")!;
        ctx.fillRect(0, y, width, ROW_PX);
      }
      const lineText = store.line(slice.line);
      const segments = rowSegments(lineText, spans.get(slice.line), slice.start, slice.end);
      ctx.globalAlpha = INK_ALPHA;
      for (const run of inkRuns(segments, tabSize, slice.indent)) {
        ctx.fillStyle = c.get(run.token ?? "text") ?? c.get("text")!;
        ctx.fillRect(PAD_PX + run.from * CELL_PX, y, (run.to - run.from) * CELL_PX, INK_PX);
      }
      ctx.globalAlpha = 1;
      for (const [from, to] of found.get(slice.line) ?? []) {
        const a = Math.max(from, slice.start);
        const b = Math.min(to, slice.end);
        if (b < a || (b === a && from !== to)) continue;
        ctx.fillStyle = c.get("match")!;
        const cells = inkCells(lineText.slice(slice.start, a), slice.indent);
        ctx.fillRect(PAD_PX + cells * CELL_PX, y - 0.5, Math.max(2, (b - a) * CELL_PX), ROW_PX);
      }
    }
    ctx.fillStyle = c.get(dragging || hovering ? "sliderActive" : "slider")!;
    ctx.fillRect(0, geo.sliderTop, width, geo.sliderHeight);
  }

  /** Cells `text` takes from a row's start (tabs to their stops). */
  function inkCells(text: string, indent: number): number {
    let cell = indent;
    for (const ch of text) cell = ch === "\t" ? (Math.floor(cell / tabSize) + 1) * tabSize : cell + 1;
    return cell;
  }

  // Everything that is drawn: once per frame, however many of these changed.
  $effect(() => {
    void [version, input, cursorLine, dragging, hovering, highlighter, brackets, decorations, width];
    if (!frame) frame = requestAnimationFrame(draw);
  });

  function yOf(e: PointerEvent): number {
    return e.clientY - canvas.getBoundingClientRect().top;
  }

  /** On the slider: drag it. Beside it: centre the text there, and keep dragging from there. */
  function onPointerDown(e: PointerEvent): void {
    if (e.button !== 0) return;
    e.preventDefault();
    canvas.setPointerCapture(e.pointerId);
    // The text's size stays as it is for the drag; only where it is scrolled to changes.
    const base = input;
    const y = yOf(e);
    let top = base.scrollTop;
    if (!onSlider(base, y)) {
      top = scrollForClick(base, y);
      onScroll(top);
    }
    const grab = y - minimapLayout({ ...base, scrollTop: top }).sliderTop;
    dragging = true;
    const onMove = (ev: PointerEvent) => onScroll(scrollForSlider(base, yOf(ev) - grab));
    const onUp = (ev: PointerEvent) => {
      dragging = false;
      // A drag that ends outside may not get its `pointerleave` while the pointer was captured.
      const r = canvas.getBoundingClientRect();
      hovering = ev.clientX >= r.left && ev.clientX <= r.right && ev.clientY >= r.top && ev.clientY <= r.bottom;
      canvas.removeEventListener("pointermove", onMove);
      canvas.removeEventListener("pointerup", onUp);
      canvas.removeEventListener("pointercancel", onUp);
    };
    canvas.addEventListener("pointermove", onMove);
    canvas.addEventListener("pointerup", onUp);
    canvas.addEventListener("pointercancel", onUp);
  }

  onMount(() => {
    const resize = new ResizeObserver(() => {
      height = canvas.clientHeight;
      width = canvas.clientWidth;
    });
    resize.observe(canvas);
    // A new theme (`<html data-theme>`, or a user's theme.css) repaints with its colours.
    const themeWatch = new MutationObserver(() => {
      palette = null;
      if (!frame) frame = requestAnimationFrame(draw);
    });
    themeWatch.observe(document.documentElement, { attributes: true });
    themeWatch.observe(document.head, { childList: true });
    return () => {
      resize.disconnect();
      themeWatch.disconnect();
      if (frame) cancelAnimationFrame(frame);
    };
  });
</script>

<div class="minimap">
  <canvas
    bind:this={canvas}
    class:dragging
    aria-hidden="true"
    onpointerdown={onPointerDown}
    onpointerenter={() => (hovering = true)}
    onpointerleave={() => (hovering = false)}
  ></canvas>
  <span class="probe" bind:this={probe} aria-hidden="true"></span>
</div>

<style>
  .minimap {
    position: relative;
    width: 100%;
    height: 100%;
    background: var(--ax-surface-1);
    border-left: 1px solid var(--ax-border);
  }

  canvas {
    display: block;
    width: 100%;
    height: 100%;
    cursor: default;
  }

  .probe {
    position: absolute;
    visibility: hidden;
    pointer-events: none;
  }
</style>
