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
  * **Vi mode (ED3) is a `ViSurface`** attached while the settings say so: it
    takes the keys, typed text and dead keys, and tells the surface the cursor's
    shape and the Visual selection to draw; scrolling and the bell it asks for
    are done here, the rest (`:w`, `]c` …) goes to the owner.
  * **The find bar (ED5.4) floats over the text** (`FindBar.svelte`): ⌘F, ⌥⌘F,
    ⌘G/⇧⌘G and ⌘E are taken here before anything else, in Vi mode too (T15);
    its matches and the "in selection" scope are drawn with the other marks.
  * **Sticky scroll and the minimap (ED5.6, T8, T9)**: the headers of the
    blocks around the top line stay pinned over the text (`editor/sticky.ts`);
    they count as covering it, so the cursor, `zt` and `H` stay below them.
    The minimap (`Minimap.svelte`) sits to the right of the scroller.
  * **Folding (ED5.5, T7)**: where the text can fold is `editor/fold/ranges.ts`,
    what is folded a `FoldState` (the owner's, kept per file, or the surface's
    own); folded lines simply have no rows in the layout. A chevron in the
    gutter, "⋯ N lines" after a folded header, ⌥⌘[ ⌥⌘] ⌥⌘0 ⌥⌘J and Vi's
    `zc zo za zM zR`. After every command whatever hides a cursor opens.
  * **`tick` is the one redraw signal.** `EditorDocument` is a plain mutable
    class, not Svelte state, so every `doc.*` call in this file is followed by
    `changed()` (which bumps `tick`); a change the owner makes behind the
    surface's back arrives through the `revision` prop instead.
-->
<script lang="ts">
  import { onMount, tick as nextTick, untrack } from "svelte";
  import type { Tree } from "web-tree-sitter";

  import { commentPrefixFor, copyText, cut, paste, run, type ClipboardText, type Command } from "../editor/commands";
  import { indentGuides, type LineDecoration, type LineDecorations } from "../editor/decorations";
  import type { EditorDocument } from "../editor/document";
  import { cursorCell, posAtCell, rowSlice, selectionRuns, uniqueByKey } from "../editor/geometry";
  import { allSelections, columnSelection, toggleCursor } from "../editor/multicursor";
  import { gutterDigits, lineLabel } from "../editor/gutter";
  import { FoldRanges, type FoldRange } from "../editor/fold/ranges";
  import { FoldState } from "../editor/fold/state";
  import {
    findKeyAction,
    foldKeyAction,
    keyAction,
    type Effect,
    type FindEffect,
    type FoldKey,
    type KeyInput,
  } from "../editor/keymap";
  import { SEVERITY_NAME, type Diagnostic, type DiagnosticSet } from "../editor/lsp/diagnostics";
  import { renderMarkdown } from "../core/markdown";
  import { FindModel } from "../editor/search/findModel";
  import { cursor, pos, range, selectionRange, type Pos, type Range } from "../editor/position";
  import { clearOfSticky, NO_STICKY, stickyAt } from "../editor/sticky";
  import { nextGrapheme, wordAt } from "../editor/text";
  import { syntaxObject, type SyntaxObjectName } from "../editor/syntax/objects";
  import { rowSegments, type Span } from "../editor/syntax/paint";
  import { VisualLayout } from "../editor/visual";
  import type { FoldAction, ViEffect } from "../editor/vi/machine";
  import { CodeActionMenu, type CodeActionPort } from "./codeActionMenu";
  import CodeActionPopup from "./CodeActionPopup.svelte";
  import { CompletionMenu, type CompletionPort } from "./completionMenu";
  import { docTouched } from "./workspaceEdit";
  import { SignatureHint, type SignaturePort } from "./signatureHint";
  import CompletionPopup from "./CompletionPopup.svelte";
  import { CursorGlide } from "./cursorGlide";
  import FindBar from "./FindBar.svelte";
  import Minimap from "./Minimap.svelte";
  import { lastFind, rememberFind } from "./findShared";
  import { KEEP_SCROLL } from "./keepScroll";
  import { searchGuard } from "./searchWorker";
  import type { SurfaceSettings } from "./surfaceSettings";
  import { viShared, viStateChanged } from "./viShared";
  import { ViSurface, type ViStatus } from "./viSurface";
  import { scrollTopFor, visibleLines } from "./viScroll";

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
    highlighter?: {
      spans(first: number, last: number, options?: { brackets?: boolean }): Map<number, Span[]>;
      /** The syntax tree, for Vi's `if`/`ac`/`ia` (V9); a highlighter without one has no such objects. */
      syntaxTree?(): Tree | null;
    } | null;
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
    /** Vi effects the surface cannot carry out itself (`:`, `ZZ`, `]c`, `gf` …). */
    onViEffect?: (effect: ViEffect) => void;
    /** Vi's mode, pending keys and recording, after every key (the mode pill, V8). */
    onViStatus?: (status: ViStatus | null) => void;
    /** Where the file lives (`root\0rel`), for Vi's file marks. */
    fileKey?: string;
    /**
     * What is folded (ED5.5): the owner's, to keep it per file or share it
     * between the two panes of a split diff. Without one the surface keeps its own.
     */
    folds?: FoldState | null;
    /** The owner's folds were opened or closed (to remember them). */
    onFolds?: () => void;
    /**
     * What a language server reported about this document (ED6, L6): underlined
     * stretches, a gutter marker per line, the messages when the mouse rests on one.
     */
    diagnostics?: DiagnosticSet | null;
    /** Also write a line's first message after its text (L6; off by default). */
    diagnosticsInline?: boolean;
    /** What the language server says about the symbol at a position (ED6.2), as Markdown. */
    hoverAt?: (at: Pos) => Promise<string | null>;
    /** ⌘-click (ED6.2, L7): go to the definition of the symbol there. */
    onDefinitionAt?: (at: Pos) => void;
    /** The language server's completion (ED6.4); without it there is no menu. */
    completion?: CompletionPort | null;
    /** The language server's signature help (ED6.6); without it there is no hint. */
    signature?: SignaturePort | null;
    /** The language server's code actions (ED6.7); without them there is no menu and no "Fix…". */
    codeActions?: CodeActionPort | null;
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
    onViEffect,
    onViStatus,
    fileKey,
    folds = null,
    onFolds,
    diagnostics = null,
    diagnosticsInline = false,
    hoverAt,
    onDefinitionAt,
    completion = null,
    signature = null,
    codeActions = null,
  }: Props = $props();

  /** Rows drawn above and below the viewport, so fast scrolling shows no gaps. */
  const OVERSCAN = 8;
  /** Space between the line numbers and the text, in cells. */
  const TEXT_GAP_CELLS = 1;
  /** A jump further than this many rows scrolls smoothly, if that is on. */
  const SMOOTH_SCROLL_ROWS = 3;
  /** Rows kept above a line jumped to with `goToLine`. */
  const GO_TO_MARGIN_ROWS = 3;
  /** Cells at the gutter's left edge for the fold chevrons. */
  const FOLD_CELLS = 2;
  /** Quiet time after a change before the gutter learns the new fold ranges. */
  const FOLD_RANGES_DELAY_MS = 150;
  /** The minimap's width, in pixels (T8). */
  const MINIMAP_W = 110;
  /** How long the lines below a fold slide when the theme's `--ax-dur-med` cannot be read. */
  const FOLD_SLIDE_FALLBACK_MS = 260;
  /** Commands that only move the selection — the ones a read-only surface allows. */
  const NON_EDITING = new Set<Command["type"]>([
    "move",
    "selectAll",
    "selectLine",
    "addCursorVertical",
    "addNextOccurrence",
    "removeLastCursor",
    "selectAllOccurrences",
    "singleCursor",
  ]);
  const NO_DECORATION: LineDecoration = {};

  let scroller: HTMLDivElement;
  let measurer: HTMLSpanElement;
  let input: HTMLTextAreaElement;

  /** Bumped after every model change; everything drawn derives from it. */
  let tick = $state(0);
  let charW = $state(8);
  let scrollTop = $state(0);
  let scrollLeft = $state(0);
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
  const gutterW = $derived((gutterCells + FOLD_CELLS) * charW);
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
  // Single values, not the `settings` object: it is new on every settings change,
  // and these effects need only rerun when what they use changed (architecture review, ED3.2).
  const wrapOn = $derived(settings.wrap);
  const tabSize = $derived(settings.tabSize);
  const fontSpec = $derived(`${settings.fontWeight} ${settings.fontSize}px "${settings.fontFamily}"`);

  $effect(() => {
    layout.setOptions({ wrap: wrapOn, width: wrapCells, tabSize });
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

  // ---------------------------------------------------------------- vi

  /** The Vi machine for this document while Vi mode is on (ED3). */
  let vi = $state.raw<ViSurface | null>(null);
  /** A short flash when Vi refuses a key (its bell). */
  let bell = $state(false);
  let bellTimer: ReturnType<typeof setTimeout> | undefined;

  /** The logical lines on screen, for `H M L` and Vi's scrolling. */
  function viewport(): { top: number; bottom: number } {
    // The pinned headers cover the lines under them: `H` starts below them (T18).
    const covered = sticky.height;
    return visibleLines(layout, { scrollTop: scrollTop + covered, viewH: viewH - covered, rowH });
  }

  /**
   * Vi on or off, as its own value: `settings` is a new object on every
   * settings change (a font size, ⌥Z), and the machine must not be rebuilt —
   * losing its mode, Visual selection and recording — for any of those.
   */
  const viOn = $derived(settings.vi);

  $effect(() => {
    if (!viOn) {
      untrack(() => onViStatus?.(null));
      return;
    }
    const attached = new ViSurface(doc, viShared(), {
      ctx: () => ({ ...ctxNow(), viewport: viewport(), folds: foldState }),
      changed,
      effect: viEffect,
      status: (s) => {
        onViStatus?.(s);
        // After every key: registers, marks and histories may have changed (saved once they settle).
        viStateChanged();
      },
      readOnly,
      fileName,
      fileKey,
      syntaxObjects: (at, name, inner, count) => {
        // Read at the moment it is asked: the highlighter arrives after the surface, and changes with the file.
        const tree = highlighter?.syntaxTree?.();
        if (!tree || !"fca".includes(name)) return null;
        return syntaxObject(tree.rootNode, doc.store, at, name as SyntaxObjectName, inner, count);
      },
    });
    vi = attached;
    untrack(() => {
      tick++;
      onViStatus?.(attached.status());
    });
    return () => {
      attached.dispose();
      vi = null;
    };
  });

  /** What Vi asks for: scrolling and the bell are the surface's; everything else the owner's. */
  function viEffect(effect: ViEffect): void {
    if (effect.type === "bell") {
      bell = true;
      clearTimeout(bellTimer);
      bellTimer = setTimeout(() => (bell = false), 150);
      return;
    }
    if (effect.type === "scrollLines") {
      if (scroller) scroller.scrollTop += effect.delta * rowH;
      return;
    }
    if (effect.type === "scroll") {
      if (!scroller) return;
      // `zt` puts the line just under the headers pinned there (T18).
      scroller.scrollTop =
        effect.to === "top"
          ? clearOfSticky(layout.firstRow(effect.line) * rowH, stickyHeight)
          : scrollTopFor(layout, { scrollTop, viewH, rowH }, effect.line, effect.to);
      return;
    }
    if (effect.type === "fold") {
      foldAction(effect.action);
      return;
    }
    onViEffect?.(effect);
  }

  // ---------------------------------------------------------------- find bar

  /** The find bar's model for this document (ED5.4); it lives as long as the document is shown. */
  let find = $state.raw<FindModel | null>(null);
  let findOpen = $state(false);
  let findReplace = $state(false);
  let findBar = $state<FindBar | null>(null);
  /** Diffs and read-only files only search (T15). */
  const canReplace = $derived(!readOnly && !decorations);

  $effect(() => {
    const model = new FindModel({
      doc,
      guard: searchGuard,
      vi: () => vi !== null,
      place: (r) => {
        if (vi) vi.placeCursor(r.start);
        else doc.setSelection({ anchor: r.start, head: r.end });
      },
      changed,
      commit: rememberFind,
    });
    find = model;
    return () => {
      model.dispose();
      find = null;
    };
  });

  /** Takes the last search (any bar's, or Vi's) into this bar before it is used. */
  function adoptLastFind(model: FindModel): void {
    const shared = lastFind();
    if (!shared) return;
    model.query = shared.query;
    model.options = { ...model.options, ...shared.options };
  }

  function runFind(effect: FindEffect): void {
    const model = find;
    if (!model) return;
    if (effect === "find" || effect === "findReplace") {
      void openFind(effect === "findReplace");
      return;
    }
    if (effect === "useSelectionForFind") {
      model.useSelection();
      return;
    }
    if (!findOpen) adoptLastFind(model);
    model.next(effect === "findPrevious");
  }

  /** ⌘F / ⌥⌘F: opens (or refocuses) the bar, with the selection as its query or scope. */
  async function openFind(withReplace: boolean): Promise<void> {
    const model = find;
    if (!model) return;
    // Open already: the query being typed stays, even if it was never used.
    if (!findOpen) adoptLastFind(model);
    model.open();
    findOpen = true;
    if (withReplace && canReplace) findReplace = true;
    tick++;
    await nextTick();
    if (withReplace && canReplace) findBar?.focusReplace();
    else findBar?.focusQuery();
  }

  function closeFind(): void {
    find?.close();
    findOpen = false;
    tick++;
    input?.focus();
  }

  // ---------------------------------------------------------------- folding

  /** What is folded: the owner's, or this surface's own (disposed with it). */
  const foldState = $derived(folds ?? new FoldState(doc));
  $effect(() => {
    const own = folds ? null : foldState;
    return () => own?.dispose();
  });
  $effect(() => {
    layout.setFolds(foldState);
    untrack(() => tick++);
  });

  /** Where the text can fold (T7): the tree, Markdown headings, indentation. */
  const foldRanges = $derived(
    new FoldRanges(doc.store, () => doc.revision, {
      tree: () => highlighter?.syntaxTree?.() ?? null,
      markdown: /\.(md|markdown)$/i.test(fileName),
      tabSize: settings.tabSize,
    }),
  );
  /**
   * The ranges the gutter offers a chevron for — caught up a moment after the
   * last change, not on every key: working them out walks the whole text.
   */
  let foldStarts = $state.raw<Map<number, number>>(new Map());
  let foldStartsTimer: ReturnType<typeof setTimeout> | undefined;

  function scheduleFoldStarts(): void {
    clearTimeout(foldStartsTimer);
    // A very long text is asked line by line (`foldableAt`), never passed over whole for the gutter.
    if (foldRanges.local) return;
    foldStartsTimer = setTimeout(() => (foldStarts = foldRanges.starts()), FOLD_RANGES_DELAY_MS);
  }

  /** Whether the gutter shows a chevron at `line` — from ranges a moment old, so drawing never waits for them. */
  function foldableAt(line: number): boolean {
    return foldRanges.local ? foldRanges.near(line) !== null : foldStarts.has(line);
  }

  // A new document, grammar or tab size: the chevrons follow soon after.
  $effect(() => {
    void foldRanges;
    void highlighter;
    untrack(scheduleFoldStarts);
    return () => clearTimeout(foldStartsTimer);
  });

  /** The lines below a fold that just closed or opened slide from where they were (T7). */
  let foldSlide = $state.raw<{ row: number; header: number; end: number; shift: number } | null>(null);
  let foldSlideTimer: ReturnType<typeof setTimeout> | undefined;

  /** ⌥⌘[ ⌥⌘] ⌥⌘0 ⌥⌘J, as the fold actions Vi's `z` commands are. */
  const FOLD_KEY_ACTIONS: Record<FoldKey, FoldAction> = {
    fold: "close",
    unfold: "open",
    foldAll: "closeAll",
    unfoldAll: "openAll",
  };

  /** Runs a fold action at the cursor's line. */
  function foldAction(action: FoldAction): void {
    const line = doc.selection.head.line;
    if (action === "closeAll") return foldsChanged(null, () => foldState.closeAll(foldRanges.all()));
    if (action === "openAll") return foldsChanged(null, () => foldState.openAll());
    const closed = foldState.closedAround(line);
    if (action === "open" || (action === "toggle" && closed)) {
      if (!closed) return;
      return foldsChanged(closed.start, () => foldState.open(closed.start));
    }
    // Close: the innermost range around the line not closed yet — on a folded header, the one around it.
    const open = foldRanges.around(line).find((r) => !foldState.closedAt(r.start));
    if (open) foldsChanged(open.start, () => foldState.close(open));
  }

  /** Opens or closes the fold with its header on `line` (a chevron, the "⋯" pill). */
  function toggleFoldAt(line: number): void {
    if (foldState.closedAt(line)) return foldsChanged(line, () => foldState.open(line));
    const r = foldRanges.local ? foldRanges.near(line) : foldRanges.at(line);
    if (r) foldsChanged(line, () => foldState.close(r));
  }

  /**
   * Carries out a change of the folds: the cursor leaves lines that are now
   * hidden (for the header of their fold), the layout is redone, and the lines
   * below `header` slide into place if the animation is on.
   */
  function foldsChanged(header: number | null, change: () => void): void {
    const rowsBefore = layout.totalRows;
    // What an opening fold hid: those lines fade in rather than slide.
    const wasHidden = header === null ? null : foldState.closedAround(header);
    change();
    const hidden = foldState.hiddenAt(doc.selection.head.line);
    if (hidden) {
      const to = hidden.from - 1;
      const at = pos(to, Math.min(doc.selection.head.col, doc.store.line(to).length));
      if (vi) vi.placeCursor(at);
      else doc.setSelection(cursor(at));
    }
    // Other cursors in a fold that closed move to its header too (one per place); the rest stay as they are.
    if (doc.extra.some((s) => foldState.hiddenAt(s.head.line))) {
      const seen = new Set([`${doc.selection.head.line}:${doc.selection.head.col}`]);
      const others = doc.extra.flatMap((s) => {
        const h = foldState.hiddenAt(s.head.line);
        const moved = h ? cursor(pos(h.from - 1, Math.min(s.head.col, doc.store.line(h.from - 1).length))) : s;
        const key = `${moved.head.line}:${moved.head.col}`;
        if (seen.has(key)) return [];
        seen.add(key);
        return [moved];
      });
      doc.setSelections(doc.selection, others);
    }
    layout.refresh();
    const rowsAdded = layout.totalRows - rowsBefore;
    if (header !== null && fx.foldAnimation && rowsAdded !== 0) {
      const row = layout.firstRow(header) + layout.rowStarts(header).length - 1;
      foldSlide = { row, header, end: rowsAdded > 0 ? (wasHidden?.end ?? header) : header, shift: -rowsAdded * rowH };
      clearTimeout(foldSlideTimer);
      foldSlideTimer = setTimeout(() => (foldSlide = null), slideMs());
    }
    tick++;
    void nextTick().then(revealCursor);
    onFolds?.();
    // The cursor may have moved to a header: the status line follows.
    onChange?.();
  }

  /** How long the slide lasts: the theme's `--ax-dur-med`, which the CSS animation runs for too. */
  function slideMs(): number {
    const value = surfaceEl ? getComputedStyle(surfaceEl).getPropertyValue("--ax-dur-med").trim() : "";
    const ms = value.endsWith("ms") ? parseFloat(value) : value.endsWith("s") ? parseFloat(value) * 1000 : NaN;
    return Number.isFinite(ms) ? ms : FOLD_SLIDE_FALLBACK_MS;
  }

  /** How a row takes part in a fold's slide: moving up or down into place, or fading in (just shown). */
  function slideOf(row: number, line: number): "slide" | "reveal" | null {
    const s = foldSlide;
    if (!s || row <= s.row) return null;
    return line > s.header && line <= s.end ? "reveal" : "slide";
  }

  /** After a command: whatever fold hides a cursor opens (a search match, an undo, a jump). */
  function revealCursors(): void {
    let opened = false;
    for (const s of [doc.selection, ...doc.extra]) opened = foldState.reveal(s.head.line) || opened;
    if (opened) onFolds?.();
  }

  // ---------------------------------------------------------------- sticky scroll

  /** The fold ranges in order, for the pinned headers — as fresh as the gutter's chevrons. */
  const stickyRanges = $derived(
    [...foldStarts].map(([start, end]): FoldRange => ({ start, end })).sort((a, b) => a.start - b.start),
  );

  /** The pinned headers' height with the view scrolled to `top`. */
  function stickyHeight(top: number): number {
    return settings.stickyScroll ? stickyAt(stickyRanges, layout, top, rowH).height : 0;
  }

  const sticky = $derived.by(() => {
    void tick;
    return settings.stickyScroll ? stickyAt(stickyRanges, layout, scrollTop, rowH) : NO_STICKY;
  });

  /** The pinned header rows as drawn: their text in colour, and the line number. */
  const stickyRows = $derived.by(() =>
    sticky.headers.map((h, i) => {
      const text = doc.store.line(h.start);
      const end = layout.rowStarts(h.start)[1] ?? text.length;
      const spans = highlighter?.spans(h.start, h.start, { brackets: fx.bracketColors }).get(h.start);
      return {
        line: h.start,
        top: i * rowH + (i === sticky.headers.length - 1 ? sticky.push : 0),
        segments: rowSegments(text, spans, 0, end),
      };
    }),
  );

  /** A click on a pinned header: to its line, with the headers around it pinned above. */
  function jumpToHeader(index: number): void {
    const header = sticky.headers[index];
    if (!header || !scroller) return;
    const text = doc.store.line(header.start);
    const at = pos(header.start, text.length - text.trimStart().length);
    scroller.scrollTop = Math.max(0, (layout.firstRow(header.start) - index) * rowH);
    if (vi) vi.placeCursor(at);
    else doc.setSelection(cursor(at));
    changed();
    input.focus();
  }

  /** Wheel over the pinned headers or the minimap scrolls the text, as it would over the text. */
  function forwardWheel(e: WheelEvent): void {
    // A wheel that counts lines (`deltaMode` 1) or pages (2), not pixels.
    const unitY = e.deltaMode === 1 ? rowH : e.deltaMode === 2 ? viewH : 1;
    const unitX = e.deltaMode === 1 ? charW : e.deltaMode === 2 ? viewW : 1;
    scroller?.scrollBy({ left: e.deltaX * unitX, top: e.deltaY * unitY });
  }

  /** Matches for the minimap: the find bar's while it is open, else Vi's. */
  function minimapMatches(first: number, last: number): Map<number, [number, number][]> {
    if (findOpen && find) return find.highlights(first, last).matches;
    return vi?.searchHighlights(first, last).matches ?? new Map();
  }

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
      diag: diagnosticMark(r),
      fold: foldMark(r),
      slide: slideOf(r.row, r.line),
      // A relative line number leaves folded lines out (T18).
      hiddenToCursor: foldState.hiddenBetween(r.line, doc.selection.head.line),
    }));
    const sel = doc.selection;
    // In Vi mode the selection drawn is the Visual one (inclusive, lines, a block);
    // otherwise the document's own.
    const viRanges = vi?.visualRanges(doc, settings.tabSize) ?? null;
    // Several cursors (ED5, T6): every one's selection, and a caret for each besides the main one.
    const others = vi ? [] : doc.extra;
    const ranges = viRanges ?? (vi ? [] : [sel, ...others].map(selectionRange));
    const runs = ranges.flatMap((r) => selectionRuns(layout, doc.store, r, first, last, settings.tabSize));
    const marks = uniqueByKey([
      ...markRuns(rows, first, last),
      ...diagnosticRuns(firstLine, lastLine, first, last),
      ...searchRuns(firstLine, lastLine, first, last),
    ]);
    const whitespace = settings.list ? whitespaceMarks(rows, first, last) : [];
    const caret = cursorCell(layout, doc.store, sel.head, settings.tabSize);
    const shape = vi?.cursorShape() ?? "bar";
    // A block cursor is as wide as the character under it (two cells for a wide one).
    const lineText = doc.store.line(sel.head.line);
    const after = pos(sel.head.line, nextGrapheme(lineText, sel.head.col));
    const next = sel.head.col < lineText.length ? cursorCell(layout, doc.store, after, settings.tabSize) : null;
    const cells = next && next.row === caret.row ? Math.max(1, next.cell - caret.cell) : 1;
    let widest = 0;
    if (!settings.wrap) for (const r of rows) widest = Math.max(widest, r.text.length);
    const current = { top: layout.firstRow(sel.head.line), rows: layout.rowStarts(sel.head.line).length };
    const extraCarets = others
      .map((s) => cursorCell(layout, doc.store, s.head, settings.tabSize))
      .filter((c) => c.row >= first && c.row <= last);
    return {
      total,
      rows,
      runs,
      marks,
      whitespace,
      caret,
      extraCarets,
      shape,
      cells,
      cursorLine: sel.head.line,
      current,
      widest,
    };
  });

  /**
   * A row's part in folding: on a line's first row the chevron (`open` if the
   * line can fold, `closed` if it is folded), and on a folded line's last row
   * the "⋯ N lines" pill, placed a cell after the text.
   */
  function foldMark(r: { line: number; sub: number; last: boolean }) {
    const closed = foldState.closedAt(r.line) ? foldState.closedAround(r.line) : null;
    const chevron = r.sub !== 0 ? null : closed ? "closed" : foldableAt(r.line) ? "open" : null;
    if (!closed || !r.last) return { chevron, pill: null };
    const end = cursorCell(layout, doc.store, pos(r.line, doc.store.line(r.line).length), settings.tabSize);
    return { chevron, pill: { cell: end.cell + 1, lines: closed.end - closed.start } };
  }

  /**
   * A row's part in the diagnostics (ED6): on a line's first row the gutter
   * marker's severity, and — with the inline setting — on its last row the
   * worst message, placed a cell after the text.
   */
  function diagnosticMark(r: { line: number; sub: number; last: boolean }) {
    const d = diagnostics?.line(r.line);
    if (!d) return null;
    const severity = SEVERITY_NAME[d.worst];
    const gutter = r.sub === 0 ? severity : null;
    if (!diagnosticsInline || !r.last) return { gutter, inline: null };
    const end = cursorCell(layout, doc.store, pos(r.line, doc.store.line(r.line).length), settings.tabSize);
    return { gutter, inline: { cell: end.cell + 2, text: d.message.split("\n")[0], severity } };
  }

  /** The underlined stretches of the lines on screen, as runs per visual row. */
  function diagnosticRuns(firstLine: number, lastLine: number, first: number, last: number) {
    const out: MarkRun[] = [];
    if (!diagnostics) return out;
    for (let line = firstLine; line <= lastLine; line++) {
      for (const m of diagnostics.line(line)?.marks ?? []) {
        pushMarkRuns(out, range(pos(line, m.from), pos(line, m.to)), `diag-${SEVERITY_NAME[m.severity]}`, first, last);
      }
    }
    return out;
  }

  type MarkRun = { key: string; row: number; from: number; to: number; kind: string };

  /** Adds the visual runs of `r` on rows `first`–`last` to `out` as marks of `kind`. */
  function pushMarkRuns(out: MarkRun[], r: Range, kind: string, first: number, last: number): void {
    for (const run of selectionRuns(layout, doc.store, r, first, last, settings.tabSize)) {
      out.push({ key: `${run.row}:${run.from}:${kind}`, row: run.row, from: run.from, to: run.to, kind });
    }
  }

  /** Where the word marks of the lines on screen fall, as runs per visual row. */
  function markRuns(rows: { line: number; sub: number; deco: LineDecoration }[], first: number, last: number) {
    const out: MarkRun[] = [];
    for (const r of rows) {
      if (r.sub !== 0 || !r.deco.marks) continue;
      for (const m of r.deco.marks) {
        pushMarkRuns(out, range(pos(r.line, m.from), pos(r.line, m.to)), m.kind, first, last);
      }
    }
    return out;
  }

  /**
   * Search matches as marks: the find bar's while it is open (with its scope
   * underneath), else Vi's (hlsearch) and the one incsearch would go to.
   */
  function searchRuns(firstLine: number, lastLine: number, first: number, last: number) {
    const out: MarkRun[] = [];
    const bar = findOpen ? find : null;
    if (bar) {
      const { matches, current, scope } = bar.highlights(firstLine, lastLine);
      if (scope) pushMarkRuns(out, scope, "find-scope", first, last);
      for (const [line, found] of matches) {
        for (const [s, e] of found) pushMarkRuns(out, range(pos(line, s), pos(line, e)), "search", first, last);
      }
      if (current) pushMarkRuns(out, current, "search-current", first, last);
      return out;
    }
    if (!vi) return out;
    const { matches, current } = vi.searchHighlights(firstLine, lastLine);
    for (const [line, found] of matches) {
      for (const [s, e] of found) pushMarkRuns(out, range(pos(line, s), pos(line, e)), "search", first, last);
    }
    if (current) pushMarkRuns(out, current, "search-current", first, last);
    return out;
  }

  /** `:set list`: a `→` on every tab and a `·` on every space that ends a line. */
  function whitespaceMarks(rows: { line: number }[], first: number, last: number) {
    const out: { key: string; row: number; from: number; glyph: string }[] = [];
    for (const line of new Set(rows.map((r) => r.line))) {
      const text = doc.store.line(line);
      const trailing = text.length - (/[ \t]*$/.exec(text)?.[0].length ?? 0);
      for (let col = 0; col < text.length; col++) {
        const ch = text[col];
        if (ch !== "\t" && !(ch === " " && col >= trailing)) continue;
        const cell = range(pos(line, col), pos(line, col + 1));
        for (const run of selectionRuns(layout, doc.store, cell, first, last, settings.tabSize)) {
          out.push({ key: `${run.row}:${run.from}`, row: run.row, from: run.from, glyph: ch === "\t" ? "→" : "·" });
        }
      }
    }
    return out;
  }

  // ---------------------------------------------------------------- model glue

  /** After a command: open folds hiding a cursor, redraw, keep the cursor in view, tell the owner. */
  function changed(reveal = true): void {
    if (reveal) revealCursors();
    layout.refresh();
    scheduleFoldStarts();
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
    // ⌘A reaches the end of the text, which may be folded; that is no reason to open it.
    changed(cmd.type !== "selectAll");
  }

  function revealCursor(): void {
    if (!scroller) return;
    // In Vi, an incsearch match being typed towards is what must be seen.
    const target = vi?.revealTarget() ?? doc.selection.head;
    const { row, cell } = cursorCell(layout, doc.store, target, settings.tabSize);
    const y = row * rowH;
    let top = scroller.scrollTop;
    // Above the view, or under the headers pinned at its top (T18): just below them.
    if (y < top + stickyHeight(top)) top = clearOfSticky(y, stickyHeight);
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
    const shape =
      view.shape === "block"
        ? { width: view.cells * charW, height: rowH, alpha: 0.45 }
        : view.shape === "underline"
          ? { width: view.cells * charW, height: 2, alpha: 1 }
          : undefined;
    const style = fx.cursor === "off" ? null : { strength: fx.cursor, glow: fx.glow, shape };
    untrack(() => glide.moveTo(target, style));
  });

  // ---------------------------------------------------------------- keyboard

  function keyInputFrom(e: KeyboardEvent): KeyInput {
    return {
      key: e.key,
      meta: e.metaKey,
      alt: e.altKey,
      shift: e.shiftKey,
      ctrl: e.ctrlKey,
      keyCode: e.keyCode,
      code: e.code,
    };
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.isComposing || composing) return;
    endHover();
    if (actionKey(e)) {
      e.preventDefault();
      return;
    }
    if (completionKey(e)) {
      e.preventDefault();
      return;
    }
    handleKeydown(e);
    // A ⌫, an arrow, Esc leaving Insert: the open menu follows the word, or closes.
    if (menu?.isOpen) followMenu();
    // A character is followed once it is in the text (`commitInput`); here only moves and deletions.
    if (hint?.isOpen && e.key.length !== 1) followHint();
  }

  function handleKeydown(e: KeyboardEvent): void {
    // The find bar's keys come first, in Vi mode as well (T15).
    const findKey = findKeyAction(keyInputFrom(e));
    if (findKey) {
      e.preventDefault();
      runFind(findKey);
      return;
    }
    // The fold keys too (T7): Vi has its own `z` commands besides.
    const foldKey = foldKeyAction(keyInputFrom(e));
    if (foldKey) {
      e.preventDefault();
      foldAction(FOLD_KEY_ACTIONS[foldKey]);
      return;
    }
    // Esc back in the text closes the bar, once there is only one cursor left to go back to —
    // in Vi, once Esc has nothing left to cancel (Normal mode, no pending keys, no command line).
    const plainEsc = e.key === "Escape" && !e.metaKey && !e.altKey && !e.shiftKey && !e.ctrlKey;
    const viIdle = (s: ViStatus) => s.mode === "normal" && !s.pending && !s.cmdline;
    if (plainEsc && findOpen && (vi ? viIdle(vi.status()) : doc.extra.length === 0)) {
      e.preventDefault();
      closeFind();
      return;
    }
    // The owner's keys (⏎ opens the file in a diff) wait while Vi takes typed text — a search being typed.
    if (!vi?.typing && interceptKey?.(e)) {
      e.preventDefault();
      return;
    }
    if (vi) {
      const result = vi.keydown(keyInputFrom(e));
      if (!result.handled) return;
      // The Mac's copy/cut/paste in Insert mode go through the native events below.
      const native = "effect" in result && ["copy", "cut", "paste"].includes(result.effect);
      if (native) return;
      e.preventDefault();
      if ("effect" in result) onEffect?.(result.effect);
      return;
    }
    const action = keyAction(keyInputFrom(e));
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
    // Outside Insert mode ⌘C is a yank (`"+y`) and never reaches here; in Insert it copies as on a Mac (V5).
    if (vi && !vi.typing) return;
    lastClip = copyText(doc);
    e.clipboardData?.setData("text/plain", lastClip.text);
    input.value = "";
  }

  function onCut(e: ClipboardEvent): void {
    e.preventDefault();
    if (readOnly || (vi && !vi.typing)) return;
    lastClip = cut(doc, ctxNow());
    e.clipboardData?.setData("text/plain", lastClip.text);
    input.value = "";
    changed();
  }

  function onPaste(e: ClipboardEvent): void {
    e.preventDefault();
    // A read-only surface still takes a paste into Vi's command line (a search).
    if (readOnly && !vi?.typing) return;
    const text = e.clipboardData?.getData("text/plain") ?? "";
    if (!text) return;
    if (vi) {
      vi.pasted(text);
      return;
    }
    // Our own last copy: it knows whether it was a whole line, and each cursor's piece (T6).
    const ours = lastClip !== null && lastClip.text === text ? lastClip : null;
    paste(doc, { text, wholeLine: ours?.wholeLine ?? false, parts: ours?.parts }, ctxNow());
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

  /** Inserts whatever the textarea holds and empties it (in Vi mode: keys, or Insert-mode text). */
  function commitInput(): void {
    const text = input.value;
    input.value = "";
    if (!text) return;
    if (vi) vi.typed(text);
    else exec({ type: "insert", text });
    if (menu && canComplete()) menu.typed(doc, text);
    else menu?.close();
    if (hint && canComplete()) hint.typed(doc, text);
    else hint?.close();
  }

  /**
   * A composition began: in Vi's Normal and Visual modes a dead key (`^`, `\``
   * on a German keyboard) is a command, taken at once and the composition
   * cancelled (V12); in Insert mode it composes as usual.
   */
  function onCompositionUpdate(e: CompositionEvent): void {
    if (!vi || !vi.deadKey(e.data)) return;
    composing = false;
    input.value = "";
    input.blur();
    input.focus();
  }

  /** Leaving mid-composition drops the unfinished marked text (a lone dead key). */
  function onBlur(): void {
    focused = false;
    menu?.close();
    hint?.close();
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

  /**
   * Runs an editor command handed in by the owner (ED6.5: a formatter's
   * changes), as a key would — then Vi's cursor follows the text.
   */
  export function applyCommand(cmd: Command): void {
    exec(cmd);
    vi?.placeCursor(doc.selection.head);
  }

  /** Where the cursor is drawn, in the surface's pixels: below its row (the rename field, L17). */
  export function cursorPoint(): { x: number; y: number } {
    const cell = cursorCell(layout, doc.store, doc.selection.head, settings.tabSize);
    return {
      x: textLeft + cell.cell * charW - (settings.wrap ? 0 : scrollLeft),
      y: (cell.row + 1) * rowH - scrollTop,
    };
  }

  // A rename changed this document from another editor (`workspaceEdit.ts`): redraw, tell the owner.
  let seenRevision = untrack(() => doc.revision);
  $effect(() =>
    docTouched.subscribe(() =>
      // Untracked: what `changed` reads must not make this effect re-subscribe on every redraw.
      untrack(() => {
        if (doc.revision === seenRevision) return;
        seenRevision = doc.revision;
        changed(false);
      }),
    ),
  );

  // ---------------------------------------------------------------- completion (ED6.4)

  /** Bumped when the menu changed, so what is drawn follows it. */
  let menuTick = $state(0);
  /** The menu for this surface's server; replaced (and the old one closed) with the server. */
  const menu = $derived(completion ? new CompletionMenu(completion, () => menuTick++) : null);
  $effect(() => {
    const current = menu;
    return () => current?.close();
  });

  /** Where the menu shows: below the start of the word, in the surface's pixels. */
  const menuView = $derived.by(() => {
    void menuTick;
    void tick;
    const view = menu?.view;
    if (!view) return null;
    const cell = cursorCell(layout, doc.store, view.anchor, settings.tabSize);
    const x = textLeft + cell.cell * charW - (settings.wrap ? 0 : scrollLeft);
    const y = (cell.row + 1) * rowH - scrollTop;
    return { ...view, x, y };
  });

  /** The chosen item's detail and documentation, fetched from the server as it is chosen. */
  let menuDoc = $state<{ detail: string; html: string | null }>({ detail: "", html: null });
  $effect(() => {
    const item = menuView?.items[menuView.selected];
    const m = menu;
    if (!item || !m) return;
    menuDoc = { detail: item.detail, html: item.documentation ? renderMarkdown(item.documentation) : null };
    let stale = false;
    void m.details(item).then((full) => {
      if (stale) return;
      menuDoc = { detail: full.detail, html: full.documentation ? renderMarkdown(full.documentation) : null };
    });
    return () => {
      stale = true;
    };
  });

  /** Typing may complete: an editable text, and in Vi only in Insert mode. */
  function canComplete(): boolean {
    return !readOnly && (!vi || vi.status().mode === "insert");
  }

  function followMenu(): void {
    if (canComplete()) menu?.follow(doc);
    else menu?.close();
  }

  /**
   * The menu's keys, before anything else (as blink.cmp's defaults in the
   * owner's Neovim): ⌃Space opens it; while open ↓/↑ and ⌃N/⌃P choose, ⌃Y
   * takes, ⌃E closes, Esc closes (in Vi it also leaves Insert). Without Vi
   * ⏎ and ⇥ take too; in Vi they stay a new line and indentation.
   */
  function completionKey(e: KeyboardEvent): boolean {
    // ⇧⌘Space: the signature of the call the cursor is in (ED6.6), as in VS Code.
    if (hint && e.code === "Space" && e.metaKey && e.shiftKey && !e.ctrlKey && !e.altKey) {
      if (canComplete()) hint.invoke(doc);
      return canComplete();
    }
    const plainEsc = e.key === "Escape" && !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey;
    // Esc closes the hint when no menu is open (in Vi it also leaves Insert, below).
    if (plainEsc && hint?.isOpen && !menu?.isOpen) {
      hint.close();
      return !vi;
    }
    const m = menu;
    if (!m) return false;
    const ctrlOnly = e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey;
    const plain = !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey;
    if (ctrlOnly && e.code === "Space") {
      if (!canComplete()) return false;
      m.invoke(doc);
      return true;
    }
    if (!m.isOpen) return false;
    if ((plain && e.key === "ArrowDown") || (ctrlOnly && e.key === "n")) m.move(1);
    else if ((plain && e.key === "ArrowUp") || (ctrlOnly && e.key === "p")) m.move(-1);
    else if ((ctrlOnly && e.key === "y") || (plain && !vi && (e.key === "Enter" || e.key === "Tab")))
      void takeCompletion();
    else if (ctrlOnly && e.key === "e") m.close();
    else if (plain && e.key === "Escape") {
      m.close();
      return !vi;
    } else return false;
    return true;
  }

  // ---------------------------------------------------------------- signature help (ED6.6)

  let hintTick = $state(0);
  const hint = $derived(signature ? new SignatureHint(signature, () => hintTick++) : null);
  $effect(() => {
    const current = hint;
    return () => current?.close();
  });

  /** The hint above the line it was asked on, at the cursor's column. */
  const hintView = $derived.by(() => {
    void hintTick;
    void tick;
    const view = hint?.view;
    if (!view) return null;
    const cell = cursorCell(layout, doc.store, doc.selection.head, settings.tabSize);
    const x = textLeft + cell.cell * charW - (settings.wrap ? 0 : scrollLeft);
    const y = cell.row * rowH - scrollTop;
    const docHtml = view.signature.documentation ? renderMarkdown(view.signature.documentation) : null;
    return { ...view.signature, x, y, docHtml };
  });

  function followHint(): void {
    if (canComplete()) hint?.follow(doc);
    else hint?.close();
  }

  /** Takes the chosen item: one step, through Vi's Insert session when Vi is on. */
  async function takeCompletion(): Promise<void> {
    const m = menu;
    if (!m) return;
    const edit = await m.take(doc);
    if (!edit || menu !== m) return;
    const cmd: Command = { type: "complete", edit };
    if (vi) vi.command(cmd);
    else exec(cmd);
  }

  function pickCompletion(index: number): void {
    const m = menu;
    const view = m?.view;
    if (!m || !view) return;
    m.move(index - view.selected);
    void takeCompletion();
  }

  // ---------------------------------------------------------------- code actions (ED6.7)

  let actionTick = $state(0);
  const actionMenu = new CodeActionMenu(() => actionTick++);
  /** Bumped on every request, so an answer that arrives after another request is dropped. */
  let actionToken = 0;

  /** Where the menu shows: below the start of the range it was asked for, in the surface's pixels. */
  const actionView = $derived.by(() => {
    void actionTick;
    void tick;
    const view = actionMenu.view;
    if (!view) return null;
    const cell = cursorCell(layout, doc.store, view.anchor, settings.tabSize);
    const x = textLeft + cell.cell * charW - (settings.wrap ? 0 : scrollLeft);
    const y = (cell.row + 1) * rowH - scrollTop;
    return { ...view, x, y };
  });

  /**
   * ⌘., ⌘L, Vi `gra` and `:action` (L21): the code actions for `from`..`to` —
   * by default the selection — in a menu below `from`. `fix` narrows them to
   * one problem's (the hover's "Fix…"). The owner says when there are none.
   */
  export async function openCodeActions(
    from: Pos = selectionRange(doc.selection).start,
    to: Pos = selectionRange(doc.selection).end,
    fix: Diagnostic | null = null,
  ): Promise<void> {
    const port = codeActions;
    if (!port) return;
    endHover();
    menu?.close();
    hint?.close();
    const token = ++actionToken;
    const items = await port.ask(from, to, fix);
    if (token !== actionToken || port !== codeActions) return;
    if (!actionMenu.show(from, items)) port.none();
  }

  /**
   * The menu's keys, before anything else while it is open (L20): ↓/↑ and
   * ⌃N/⌃P choose, ⏎ takes, 1–9 take that entry, other characters narrow the
   * list and ⌫ widens it again, Esc closes. Any other key closes it and goes
   * on as usual.
   */
  function actionKey(e: KeyboardEvent): boolean {
    if (!actionMenu.isOpen) return false;
    const plain = !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey;
    const ctrlOnly = e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey;
    if ((plain && e.key === "ArrowDown") || (ctrlOnly && e.key === "n")) actionMenu.move(1);
    else if ((plain && e.key === "ArrowUp") || (ctrlOnly && e.key === "p")) actionMenu.move(-1);
    else if (plain && e.key === "Enter") takeAction(null);
    else if (plain && e.key === "Escape") actionMenu.close();
    else if (plain && e.key === "Backspace") actionMenu.narrow(null);
    else if (["Shift", "Alt", "Meta", "Control", "CapsLock"].includes(e.key)) return true;
    else if (e.metaKey || e.ctrlKey || e.key.length !== 1) {
      actionMenu.close();
      return false;
    } else if (/^[1-9]$/.test(e.key)) takeAction(Number(e.key));
    else actionMenu.narrow(e.key);
    return true;
  }

  /** Takes the chosen entry (or entry `number`) and hands it to the owner to carry out. */
  function takeAction(number: number | null): void {
    const item = actionMenu.take(number);
    if (item) codeActions?.take(item);
  }

  function pickAction(index: number): void {
    takeAction(index + 1);
  }

  // A new server (or none) takes its menu along.
  $effect(() => {
    void codeActions;
    return () => actionMenu.close();
  });

  // ---------------------------------------------------------------- hover (ED6)

  /** How long the mouse rests on a symbol before what is known about it shows. */
  const HOVER_DELAY_MS = 350;
  let hoverTimer: ReturnType<typeof setTimeout> | null = null;
  /** Bumped on every move, so an answer that arrives after the mouse moved on is dropped. */
  let hoverToken = 0;
  /** How long a hover with a "Fix…" button stays after the mouse left, to reach the button. */
  const HOVER_GRACE_MS = 400;
  let hoverGrace: ReturnType<typeof setTimeout> | null = null;
  /**
   * The problems and the server's note (sanitised HTML) under the mouse, and
   * where to show them; `fix` is the problem the "Fix…" button asks code actions
   * for (L21) — the hover can then be clicked.
   */
  let hover = $state<{
    x: number;
    y: number;
    items: { severity: string; text: string }[];
    html: string | null;
    fix: Diagnostic | null;
  } | null>(null);

  function onHoverMove(e: MouseEvent): void {
    // A hover with a button waits a moment, so the mouse can travel to it.
    if (hover?.fix) return leaveHoverSoon();
    endHover();
    if (e.buttons !== 0 || (!hoverAt && (!diagnostics || diagnostics.size === 0))) return;
    const { clientX, clientY } = e;
    const rect = surfaceEl.getBoundingClientRect();
    const token = hoverToken;
    hoverTimer = setTimeout(() => {
      void showHover(posAt({ clientX, clientY } as MouseEvent), clientX - rect.left, clientY - rect.top + rowH, token);
    }, HOVER_DELAY_MS);
  }

  /** Shows what is known about `at` at (`x`, `y`) in the surface, unless the mouse moved on meanwhile. */
  async function showHover(at: Pos, x: number, y: number, token: number): Promise<void> {
    const problems = diagnostics?.at(at) ?? [];
    const items = problems.map((d) => ({
      severity: SEVERITY_NAME[d.severity],
      text: `${d.message}${d.source ? ` (${d.source}${d.code ? ` ${d.code}` : ""})` : ""}`,
    }));
    const markdown = hoverAt ? await hoverAt(at) : null;
    if (token !== hoverToken) return;
    // The server's text is foreign content: rendered through DOMPurify (`renderMarkdown`).
    const html = markdown ? renderMarkdown(markdown) : null;
    if (items.length === 0 && !html) return;
    hover = { x, y, items, html, fix: codeActions ? (problems[0] ?? null) : null };
  }

  /** Vi's `K` (ED6.2): what is known about the symbol under the cursor, shown below it. */
  export function showHoverAtCursor(): void {
    endHover();
    const at = doc.selection.head;
    const cell = cursorCell(layout, doc.store, at, settings.tabSize);
    const x = textLeft + cell.cell * charW - (settings.wrap ? 0 : scrollLeft);
    const y = (cell.row + 1) * rowH - scrollTop;
    void showHover(at, x, y, hoverToken);
  }

  /** The mouse left a hover that has a button: it goes unless the mouse reaches it in time. */
  function leaveHoverSoon(): void {
    hoverGrace ??= setTimeout(endHover, HOVER_GRACE_MS);
  }

  function stayHover(): void {
    if (hoverGrace) clearTimeout(hoverGrace);
    hoverGrace = null;
  }

  /** The hover's "Fix…": the code actions for its problem (L21). */
  function fixFromHover(): void {
    const problem = hover?.fix;
    endHover();
    input.focus();
    if (problem) void openCodeActions(problem.start, problem.end, problem);
  }

  function endHover(): void {
    stayHover();
    if (hoverTimer) clearTimeout(hoverTimer);
    hoverTimer = null;
    hoverToken++;
    hover = null;
  }

  // Nothing may show up after the surface is gone.
  $effect(() => () => endHover());

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
    menu?.close();
    actionMenu.close();
    hint?.close();
    if (e.button !== 0) return;
    e.preventDefault();
    input.focus();
    const unit = e.detail >= 3 ? lineRange : e.detail === 2 ? wordRange : null;
    const start = posAt(e);
    if (e.metaKey && !e.altKey && !e.shiftKey && onDefinitionAt && !unit) return onDefinitionAt(start);
    if (vi) return viMousedown(start);
    if (e.altKey && !unit) return altMousedown(start);
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

  /**
   * ⌥-click adds a cursor there, or takes away the one that is there (ED5,
   * T6); ⌥-drag draws a column of selections from where it began.
   */
  function altMousedown(start: Pos): void {
    // The cursors from before this click: a drag adds its column to them.
    const before = allSelections(doc);
    toggleCursor(doc, start);
    changed();
    let dragged = false;
    const onMove = (ev: MouseEvent) => {
      const at = posAt(ev);
      if (!dragged && at.line === start.line && at.col === start.col) return;
      dragged = true;
      columnSelection(doc, start, at, settings.tabSize, before.length > 1 ? before : []);
      changed();
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  /** Vi mode: a click puts the cursor there; a drag selects in Visual mode. */
  function viMousedown(start: Pos): void {
    vi?.placeCursor(start);
    changed();
    let dragged = false;
    const onMove = (ev: MouseEvent) => {
      const at = posAt(ev);
      if (!dragged && at.line === start.line && at.col === start.col) return;
      dragged = true;
      vi?.selectVisual(start, at);
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
    void document.fonts
      .load(fontSpec)
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
    goTo(pos(line, 0));
  }

  /** {@link goToLine} to a column: the next problem (ED6, F8 / `]d`). */
  export function goTo(at: Pos): void {
    const target = Math.max(0, Math.min(at.line, doc.store.lineCount() - 1));
    const col = Math.max(0, Math.min(at.col, doc.store.line(target).length));
    doc.setSelection({ anchor: pos(target, col), head: pos(target, col) });
    layout.refresh();
    tick++;
    onChange?.();
    // After the next render: a freshly opened file's sizer is not yet as tall
    // as the text, and a scroll past its end would be cut short to 0.
    void nextTick().then(() => {
      if (!scroller) return;
      const y = layout.firstRow(target) * rowH;
      // A few rows of what leads up to it — but never with the line under the pinned headers.
      const top = Math.min(Math.max(0, y - GO_TO_MARGIN_ROWS * rowH), clearOfSticky(y, stickyHeight));
      const far = Math.abs(top - scroller.scrollTop) > SMOOTH_SCROLL_ROWS * rowH;
      scroller.scrollTo({ top, behavior: fx.smoothScroll && far ? "smooth" : "instant" });
    });
  }
</script>

<div
  class="surface"
  class:bell
  class:focused
  class:glow={fx.glow}
  bind:this={surfaceEl}
  style:--cell="{charW}px"
  style:--row="{rowH}px"
  style:--fold-shift="{foldSlide?.shift ?? 0}px"
  style:font-family={`"${settings.fontFamily}", var(--ax-font-mono)`}
  style:font-weight={settings.fontWeight}
  style:font-size="{settings.fontSize}px"
  style:font-variant-ligatures={settings.ligatures ? "normal" : "none"}
  style:--fold-cells={FOLD_CELLS}
  style:--minimap-w="{settings.minimap ? MINIMAP_W : 0}px"
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
      scrollLeft = scroller.scrollLeft;
      onTopLine?.(layout.lineAt(Math.floor(scrollTop / rowH)).line);
      onScrollPos?.(scroller.scrollTop, scroller.scrollLeft);
    }}
    onmousedown={onMousedown}
    onmousemove={onHoverMove}
    onmouseleave={() => (hover?.fix ? leaveHoverSoon() : endHover())}
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
            <div
              class="number decorated ln-{r.deco.kind ?? 'none'} {r.slide ?? ''}"
              style:top="{r.row * rowH}px"
            >
              {r.sub === 0 ? (r.deco.gutter ?? "") : ""}
            </div>
          {:else if r.sub === 0}
            <div
              class="number {r.slide ?? ''}"
              class:current={r.line === view.cursorLine}
              style:top="{r.row * rowH}px"
            >
              {lineLabel(r.line, view.cursorLine, settings.lineNumbers, r.hiddenToCursor)}
            </div>
          {/if}
          {#if r.diag?.gutter}
            <div class="diag-dot diag-{r.diag.gutter}" style:top="{r.row * rowH}px" aria-hidden="true"></div>
          {/if}
          {#if r.fold.chevron}
            <button
              type="button"
              tabindex="-1"
              class="fold-chevron"
              class:closed={r.fold.chevron === "closed"}
              style:top="{r.row * rowH}px"
              title={r.fold.chevron === "closed" ? "Unfold (⌥⌘])" : "Fold (⌥⌘[)"}
              aria-label={r.fold.chevron === "closed" ? "Unfold" : "Fold"}
              onmousedown={(e) => {
                e.preventDefault();
                e.stopPropagation();
                toggleFoldAt(r.line);
                input.focus();
              }}>›</button
            >
          {/if}
        {/each}
      </div>
      <div class="content" style:left="{textLeft}px">
        {#each view.rows as r (r.row)}
          {#if r.deco.kind}
            <div class="line-bg ln-{r.deco.kind} {r.slide ?? ''}" style:top="{r.row * rowH}px"></div>
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
            <div class="guide {r.slide ?? ''}" style:top="{r.row * rowH}px" style:left="{cell * charW}px"></div>
          {/each}
        {/each}
        {#each view.whitespace as w (w.key)}
          <div class="ws" style:top="{w.row * rowH}px" style:left="{w.from * charW}px" style:width="{charW}px">
            {w.glyph}
          </div>
        {/each}
        {#each view.runs as run, i (i)}
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
            class="row {r.slide ?? ''}"
            style:top="{r.row * rowH}px"
            style:padding-left="{r.indent * charW}px"
          >{#each r.segments as seg, i (i)}{#if seg.token}<span class="tk-{seg.token}">{seg.text}</span
              >{:else}{seg.text}{/if}{/each}</div
          >
          {#if r.fold.pill}
            <button
              type="button"
              tabindex="-1"
              class="fold-pill {r.slide ?? ''}"
              style:top="{r.row * rowH}px"
              style:left="{r.fold.pill.cell * charW}px"
              title="Unfold (⌥⌘])"
              onmousedown={(e) => {
                e.preventDefault();
                e.stopPropagation();
                toggleFoldAt(r.line);
                input.focus();
              }}>⋯ {r.fold.pill.lines} {r.fold.pill.lines === 1 ? "line" : "lines"}</button
            >
          {/if}
          {#if r.diag?.inline}
            <div
              class="diag-inline diag-{r.diag.inline.severity}"
              style:top="{r.row * rowH}px"
              style:left="{r.diag.inline.cell * charW}px"
            >
              {r.diag.inline.text}
            </div>
          {/if}
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
        {#each view.extraCarets as c, i (i)}
          <div
            class="caret extra"
            class:hidden={!focused || composing}
            style:top="{c.row * rowH}px"
            style:left="{c.cell * charW}px"
          ></div>
        {/each}
        {#key tick}
          <div
            class="caret {view.shape}"
            class:hidden={!focused || composing || gliding}
            style:top="{view.caret.row * rowH}px"
            style:left="{view.caret.cell * charW}px"
            style:--cells={view.cells}
          ></div>
        {/key}
        <!-- In Vi mode a read-only surface still takes typed keys (`j`, `]c`) through the
             textarea (V12); the machine refuses every change there itself. -->
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
          readonly={readOnly && !vi}
          onkeydown={onKeydown}
          oninput={onInput}
          oncompositionstart={() => (composing = true)}
          oncompositionupdate={onCompositionUpdate}
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
  {#if stickyRows.length > 0}
    <!-- The headers of the blocks around the top line (T9); a click goes to one. -->
    <div class="sticky" style:height="{sticky.height}px" onwheel={forwardWheel} role="presentation">
      {#each stickyRows as h, i (h.line)}
        <button
          type="button"
          tabindex="-1"
          class="sticky-row"
          style:top="{h.top}px"
          title="Go to line {h.line + 1}"
          onmousedown={(e) => {
            e.preventDefault();
            jumpToHeader(i);
          }}
        >
          <span class="sticky-gutter" style:width="{gutterW}px">
            {#if !decorations && settings.lineNumbers !== "off"}{h.line + 1}{/if}
          </span>
          <span class="sticky-text" style:left="{textLeft - (settings.wrap ? 0 : scrollLeft)}px"
            >{#each h.segments as seg, k (k)}{#if seg.token}<span class="tk-{seg.token}">{seg.text}</span
                >{:else}{seg.text}{/if}{/each}</span
          >
        </button>
      {/each}
    </div>
  {/if}
  {#if settings.minimap}
    <div class="minimap-slot" style:width="{MINIMAP_W}px" onwheel={forwardWheel} role="presentation">
      <Minimap
        {layout}
        store={doc.store}
        version={tick}
        {rowH}
        {viewH}
        {scrollTop}
        tabSize={settings.tabSize}
        cursorLine={view.cursorLine}
        {highlighter}
        brackets={fx.bracketColors}
        matches={minimapMatches}
        {decorations}
        onScroll={(top) => scroller?.scrollTo({ top, behavior: "instant" })}
      />
    </div>
  {/if}
  <canvas class="glide" class:on={gliding} bind:this={canvas} aria-hidden="true"></canvas>
  {#if hover}
    <div
      class="diag-hover"
      class:interactive={hover.fix !== null}
      style:left="{hover.x}px"
      style:top="{hover.y}px"
      role="tooltip"
      onmouseenter={stayHover}
      onmouseleave={endHover}
    >
      {#each hover.items as item, i (i)}
        <p class="diag-{item.severity}">{item.text}</p>
      {/each}
      {#if hover.fix}
        <button type="button" class="fix" onmousedown={(e) => e.preventDefault()} onclick={fixFromHover}>
          Fix…
        </button>
      {/if}
      {#if hover.html}
        <!-- Sanitised by `renderMarkdown` (DOMPurify): the text comes from the language server. -->
        <div class="hover-doc">{@html hover.html}</div>
      {/if}
    </div>
  {/if}
  {#if hintView}
    <div class="signature-hint" style:left="{hintView.x}px" style:top="{hintView.y}px" role="tooltip">
      <code
        >{hintView.before}<mark>{hintView.active}</mark>{hintView.after}</code
      >{#if hintView.overloads}<span class="overloads">{hintView.overloads}</span>{/if}
      {#if hintView.docHtml}
        <!-- Sanitised by `renderMarkdown` (DOMPurify): the text comes from the language server. -->
        <div class="hover-doc">{@html hintView.docHtml}</div>
      {/if}
    </div>
  {/if}
  {#if actionView}
    <CodeActionPopup
      items={actionView.items}
      selected={actionView.selected}
      query={actionView.query}
      x={actionView.x}
      y={actionView.y}
      onPick={pickAction}
    />
  {/if}
  {#if menuView}
    <CompletionPopup
      items={menuView.items}
      selected={menuView.selected}
      x={menuView.x}
      y={menuView.y}
      detail={menuDoc.detail}
      docHtml={menuDoc.html}
      onPick={pickCompletion}
    />
  {/if}
  {#if findOpen && find}
    <FindBar
      bind:this={findBar}
      model={find}
      version={tick}
      {canReplace}
      showReplace={findReplace}
      onToggleReplace={() => (findReplace = !findReplace)}
      onClose={closeFind}
    />
  {/if}
</div>

<style>
  .surface {
    position: relative;
    /* The layers inside (gutter, caret, glide) stack here, not over the owner's own overlays. */
    isolation: isolate;
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
    right: var(--minimap-w);
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
    left: calc(var(--fold-cells) * var(--cell));
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

  /* Vi's search (ED3.3): every match, and the one a search being typed goes to. */
  .mk-search {
    background: var(--ax-search-match);
  }

  .mk-search-current {
    background: var(--ax-search-current);
  }

  /* Language-server diagnostics (ED6, L6): a wavy line under the stretch, in the
     severity's colour; a hint only a dotted one. */
  .mk-diag-error {
    --diag: var(--ax-diag-error);
  }
  .mk-diag-warning {
    --diag: var(--ax-diag-warning);
  }
  .mk-diag-info {
    --diag: var(--ax-diag-info);
  }
  .mk-diag-error,
  .mk-diag-warning,
  .mk-diag-info {
    border-radius: 0;
    background-image:
      linear-gradient(45deg, transparent 65%, var(--diag) 80%, transparent 90%),
      linear-gradient(135deg, transparent 5%, var(--diag) 15%, transparent 25%),
      linear-gradient(135deg, transparent 45%, var(--diag) 55%, transparent 65%),
      linear-gradient(45deg, transparent 35%, var(--diag) 40%, transparent 55%);
    background-size: var(--ax-diag-wave);
    background-repeat: repeat-x;
    background-position: left bottom;
  }
  .mk-diag-hint {
    border-radius: 0;
    border-bottom: 1px dotted var(--ax-diag-hint);
  }

  /* The gutter marker of a line with a problem, at the gutter's left edge. */
  .diag-dot {
    position: absolute;
    left: var(--ax-space-1);
    width: var(--ax-diag-dot);
    height: var(--ax-diag-dot);
    margin-top: calc((var(--row) - var(--ax-diag-dot)) / 2);
    border-radius: 50%;
    pointer-events: none;
  }
  .diag-dot.diag-error {
    background: var(--ax-diag-error);
  }
  .diag-dot.diag-warning {
    background: var(--ax-diag-warning);
  }
  .diag-dot.diag-info {
    background: var(--ax-diag-info);
  }
  .diag-dot.diag-hint {
    background: var(--ax-diag-hint);
  }

  /* "Message at line end" (off by default): the line's worst message after its text. */
  .diag-inline {
    position: absolute;
    height: var(--row);
    line-height: var(--row);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
    white-space: nowrap;
    opacity: 0.8;
    pointer-events: none;
  }
  .diag-inline.diag-error {
    color: var(--ax-diag-error);
  }
  .diag-inline.diag-warning {
    color: var(--ax-diag-warning);
  }
  .diag-inline.diag-info {
    color: var(--ax-diag-info);
  }
  .diag-inline.diag-hint {
    color: var(--ax-diag-hint);
  }

  /* Signature help (ED6.6): above the line being typed, the active parameter marked. */
  .signature-hint {
    position: absolute;
    z-index: 7;
    max-width: min(80ch, 90%);
    transform: translateY(-100%);
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    box-shadow: var(--ax-shadow-pop);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text);
    pointer-events: none;
  }
  .signature-hint code {
    font-family: var(--ax-font-mono);
    white-space: pre-wrap;
  }
  .signature-hint mark {
    background: none;
    color: var(--ax-accent);
    font-weight: 600;
    text-decoration: underline;
  }
  .signature-hint .overloads {
    margin-left: var(--ax-space-2);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  .signature-hint .hover-doc {
    max-height: 30vh;
    overflow: hidden;
    margin-top: var(--ax-space-1);
    padding-top: var(--ax-space-1);
    border-top: 1px solid var(--ax-border);
    font-family: var(--ax-font-sans);
  }
  .signature-hint .hover-doc :global(p) {
    margin: var(--ax-space-1) 0;
  }
  .signature-hint .hover-doc :global(pre) {
    margin: var(--ax-space-1) 0;
    font-family: var(--ax-font-mono);
    white-space: pre-wrap;
  }

  /* The messages under a resting mouse. */
  .diag-hover {
    position: absolute;
    z-index: 6;
    max-width: min(60ch, 80%);
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    box-shadow: var(--ax-shadow-pop);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text);
    pointer-events: none;
  }
  .diag-hover.interactive {
    pointer-events: auto;
  }
  .diag-hover .fix {
    margin-top: var(--ax-space-1);
    padding: 0 var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-1);
    color: var(--ax-accent);
    font: inherit;
    cursor: pointer;
  }
  .diag-hover .fix:hover {
    background: var(--ax-accent-muted);
  }
  .diag-hover p {
    margin: 0;
    padding-left: var(--ax-space-2);
    border-left: 2px solid var(--ax-diag-hint);
    white-space: pre-wrap;
  }
  .diag-hover p + p {
    margin-top: var(--ax-space-1);
  }
  .diag-hover .hover-doc {
    max-height: 40vh;
    overflow: hidden;
  }
  .diag-hover p + .hover-doc {
    margin-top: var(--ax-space-2);
    padding-top: var(--ax-space-2);
    border-top: 1px solid var(--ax-border);
  }
  .diag-hover .hover-doc :global(pre) {
    margin: var(--ax-space-1) 0;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    white-space: pre-wrap;
  }
  .diag-hover .hover-doc :global(p) {
    margin: var(--ax-space-1) 0;
    padding: 0;
    border: 0;
  }
  .diag-hover p.diag-error {
    border-left-color: var(--ax-diag-error);
  }
  .diag-hover p.diag-warning {
    border-left-color: var(--ax-diag-warning);
  }
  .diag-hover p.diag-info {
    border-left-color: var(--ax-diag-info);
  }

  /* The find bar's "in selection" range (ED5.4): a faint wash under its matches. */
  .mk-find-scope {
    background: var(--ax-search-scope);
    border-radius: 0;
  }

  /* `:set list`: tabs and trailing spaces, faint, under the text's own layer. */
  .ws {
    position: absolute;
    height: var(--row);
    line-height: var(--row);
    text-align: center;
    color: var(--ax-editor-whitespace);
    pointer-events: none;
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

  /* Folding (ED5.5): a chevron at the gutter's left edge — shown while the gutter is
     hovered, always on a folded line — and the "⋯ N lines" pill after a folded header. */
  .fold-chevron {
    position: absolute;
    left: 0;
    width: calc(var(--fold-cells) * var(--cell));
    height: var(--row);
    padding: 0;
    border: 0;
    background: none;
    color: var(--ax-text-muted);
    font: inherit;
    line-height: var(--row);
    text-align: center;
    cursor: pointer;
    opacity: 0;
    transform: rotate(90deg);
    transition:
      opacity var(--ax-dur-fast) var(--ax-ease),
      transform var(--ax-dur-fast) var(--ax-ease);
  }

  .gutter:hover .fold-chevron,
  .fold-chevron.closed {
    opacity: 1;
  }

  .fold-chevron.closed {
    color: var(--ax-accent);
    transform: none;
  }

  .fold-chevron:hover {
    color: var(--ax-accent);
  }

  .fold-pill {
    position: absolute;
    height: calc(var(--row) - 4px);
    margin-top: 2px;
    padding: 0 var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
    line-height: 1;
    white-space: nowrap;
    cursor: pointer;
  }

  .fold-pill:hover {
    border-color: var(--ax-accent);
    color: var(--ax-accent);
  }

  /* The slide (T7): the lines below a fold start where they were and move into place;
     the lines an opening fold shows fade in. */
  .slide {
    animation: fold-slide var(--ax-dur-med) var(--ax-ease);
  }

  .reveal {
    animation: fold-reveal var(--ax-dur-med) var(--ax-ease);
  }

  @keyframes fold-slide {
    from {
      transform: translateY(var(--fold-shift));
    }
  }

  @keyframes fold-reveal {
    from {
      opacity: 0;
    }
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

  /* Sticky scroll (T9): the pinned headers, over the text and below the cursor's glide. */
  .sticky {
    position: absolute;
    top: 0;
    left: 0;
    right: var(--minimap-w);
    z-index: 3;
    overflow: hidden;
    background: var(--ax-surface-1);
    box-shadow: var(--ax-sticky-shadow);
  }

  .sticky-row {
    position: absolute;
    left: 0;
    right: 0;
    height: var(--row);
    padding: 0;
    border: 0;
    background: var(--ax-surface-1);
    color: var(--ax-text);
    font: inherit;
    line-height: var(--row);
    text-align: left;
    white-space: pre;
    cursor: pointer;
  }

  .sticky-row:hover {
    background: var(--ax-surface-2);
  }

  .sticky-gutter {
    position: absolute;
    left: 0;
    z-index: 1;
    height: var(--row);
    padding-right: var(--cell);
    box-sizing: border-box;
    background: inherit;
    color: var(--ax-text-muted);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .sticky-text {
    position: absolute;
  }

  .minimap-slot {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
  }

  /* Sized to the scroller's client area by `cursorGlide.ts` (not the minimap, not a scrollbar). */
  .glide {
    position: absolute;
    top: 0;
    left: 0;
    z-index: 4;
    pointer-events: none;
    visibility: hidden;
  }

  .glide.on {
    visibility: visible;
  }

  .glow .caret {
    box-shadow: 0 0 16px 3px var(--ax-editor-glow);
  }

  .glow .number.current {
    text-shadow: 0 0 14px var(--ax-editor-glow);
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

  /* The cursors besides the main one (T6): the same bar, a touch fainter. */
  .caret.extra {
    opacity: 0.7;
  }

  /* Vi's cursors (V8): a see-through block over the character, an underline in Replace. */
  .caret.block {
    width: calc(var(--cells, 1) * var(--cell));
    opacity: 0.45;
  }

  .caret.underline {
    width: calc(var(--cells, 1) * var(--cell));
    height: 2px;
    margin-top: calc(var(--row) - 2px);
  }

  /* Vi's bell: a brief outline instead of a sound. */
  .surface.bell {
    box-shadow: inset 0 0 0 1px var(--ax-accent);
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
