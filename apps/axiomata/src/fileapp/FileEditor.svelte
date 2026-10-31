<!--
  One file in the editor, with everything that goes with editing it
  (`docs/plans/editor.md`, F7–F10, G4, G8): the session's banners, the surface,
  the Markdown preview, the "Compare" diff, the status line, and the timers for
  recovery and autosave. Shared by the full-screen file app and the IDE's file
  pane (`docs/plans/git-layer.md`, H5, H14) — the editor appears in several
  places (D12) and edits the same way in each.

  * **All file logic is in `FileSession`** (tested without a DOM); this
    component only shows its banner and state and forwards the owner's choices.
  * **The owner opens files** (`open`): it decides what a failed open means
    (the file app forgets a recent entry, a pane says the file is gone).
  * **Unsaved text is kept aside two seconds after the last change** (F8), and
    whenever a file is left for another one or the component goes away.
  * **Autosave (F9)** saves 1 s after the last change, or when the editor is
    hidden or the window loses focus — always with the expected version, so it
    stops at a conflict instead of overwriting.
  * **Every kind of file the file app shows** (ED4, W1, W3): text and code in the
    editor; Markdown, HTML and SVG with a rendered view beside or instead of
    the source (⌘⇧V), opened on it when the owner opens to read; raster images
    as a picture; and a new note (`newNote`) that ⌘S files with `create_note`.
  * **A note's wikilinks and tags** (`docs/plans/tags-links.md`, TL2): coloured
    in the source, links in the reading view; ⌘-click, Vi's `gd` or a click
    there follows a link (a link into nothing creates its note beside this
    one), and every jump into another file is kept in the app-wide history
    that ⌃- / ⌃⇧- walk (`jumpHistory.ts`). `[[` and `#` complete.
-->
<script lang="ts" module>
  /** What the owner shows about the open file (a title, the unsaved dot). */
  export interface OpenFileState {
    root: string;
    rel: string;
    dirty: boolean;
    /** A new note, not filed yet (W4). */
    untitled: boolean;
  }

  /** How `open` went; a failure keeps the file that was open before. */
  export type OpenResult = { ok: true } | { ok: false; kind: string | null; message: string };
</script>

<script lang="ts">
  import { formatDateTime, formatNumber } from "../core/format";
  import { keyHints } from "../core/keyHints";
  import { get } from "svelte/store";
  import BreakpointEditor from "../ide/BreakpointEditor.svelte";
  import { breakpointInfo, breakpoints, infoOf, linesOf } from "../ide/breakpoints";
  import { isDebuggable } from "../ide/debugBackend";
  import { editDebugBreakpoint, execPoint, moveDebugBreakpoints, toggleDebugBreakpoint } from "../ide/debug";
  import { onMount, tick as nextTick, untrack, type Snippet } from "svelte";

  import { invokeBackend, listenBackend, type FileChange } from "../core/backend";
  import { emit } from "../core/bus";
  import type { NoteLinkRendering } from "../core/markdown";
  import { messageOf } from "../core/errors";
  import { hunksFromTexts } from "../editor/diff/hunks";
  import { DiffModel, textLines } from "../editor/diff/model";
  import type { Indent } from "../editor/detect";
  import type { Effect } from "../editor/keymap";
  import type { SyntaxHighlighter } from "../editor/syntax/highlighter";
  import { symbolsOf, type OutlineInfo } from "./outlineModel";
  import type { OutlineSymbol } from "../editor/syntax/outline";
  import { createFile, fileBackend, type FileRemoved, type Formatted, type FileRenamed } from "./backend";
  import { jumpHistory, jumpKey, type JumpPlace } from "./jumpHistory";
  import { noteCompletion } from "./noteCompletion";
  import { noteMarkAt, withNoteMarks } from "./noteHighlight";
  import {
    invalidateNoteIndex,
    loadNoteIndex,
    loadTagList,
    onNoteIndex,
    peekNoteIndex,
    rootRel,
    vaultPrefix,
    vaultRel,
  } from "./noteIndex";
  import { fileStem, headingLine, newNotePath, parseWikilink, type NoteMark } from "./noteLinks";
  import { markDirty } from "./dirtyFiles";
  import { foldKey, rememberedFolds, rememberFolds, updateRememberedFolds } from "./foldMemory";
  import DiffPanes from "./DiffPanes.svelte";
  import { editorFace } from "./editorFace.svelte";
  import { editorSettings, ensureEditorSettingsLoaded, formatsOnSave, updateEditorSettings } from "./editorSettings";
  import { chord, foreignModifier, platform } from "../core/keys";
  import { applyPanelZoomStep, panelZoomStep, previewZoom } from "./panelZoomKeys";
  import { applyTextEdits, storeBody, textChanges } from "../editor/textEdits";
  import { applyWorkspaceEdit, type EditPorts } from "./workspaceEdit";
  import { wordAt } from "../editor/text";
  import EditorSurface from "./EditorSurface.svelte";
  import {
    initialViewMode,
    isImagePath,
    nextViewMode,
    previewKindFor,
    viewModeLabel,
    type OpenIntent,
    type PreviewKind,
    type ViewMode,
  } from "./fileKinds";
  import HtmlPreview from "./HtmlPreview.svelte";
  import ImageView from "./ImageView.svelte";
  import { highlightFor } from "./highlighting";
  import MarkdownPreview from "./MarkdownPreview.svelte";
  import { ScrollLink } from "./scrollLink";
  import { DRAFT_REL, DRAFT_ROOT, FileSession, FOREIGN_ROOT } from "./session";
  import { trailAuthor, type EditorTrail } from "./trailAuthor";
  import { agentStatus } from "../ide/agentStatus";
  import { session as projectSession } from "../ide/projectSession";
  import { harnessBadge, harnessLabel } from "../ide/rosterLook";
  import { statusParts } from "./status";
  import { DiagnosticSet } from "../editor/lsp/diagnostics";
  import { detectLanguage } from "../editor/syntax/languages";
  import { definitionFile, openOnServer, type LspDocument } from "./lsp";
  import { buildLocationList, type LocationList } from "./locationList";
  import type { CodeActionPort } from "./codeActionMenu";
  import type { CompletionPort } from "./completionMenu";
  import type { SignaturePort } from "./signatureHint";
  import { parseWorkspaceEdit, type Location, type LocationKind } from "../editor/lsp/client";
  import { fixesOnly, type CodeActionItem } from "../editor/lsp/codeActions";
  import type { Diagnostic } from "../editor/lsp/diagnostics";
  import { toast } from "../core/toast";
  import { surfaceSettings, wrapsByDefault } from "./surfaceSettings";
  import SvgPreview from "./SvgPreview.svelte";
  import { isUnder, renamedPath } from "./treeModel";
  import { applySet } from "./viOptions";
  import ViStatusLine from "./ViStatusLine.svelte";
  import { tEditor, type EditorText } from "../i18n/editor";
  import type { ViStatus } from "./viSurface";
  import type { GutterMode } from "../editor/gutter";
  import type { ViEffect } from "../editor/vi/machine";

  interface Props {
    /** On screen: leaving it is a moment for "autosave when leaving". */
    visible?: boolean;
    /** A hint above the text, from the owner (G6: the agent is working in this worktree). */
    notice?: string | null;
    /** ⌘O — the owner decides what opening another file means. */
    onOpenRequest?: () => void;
    /** Vi's `ZZ`/`ZQ` (and `:q` with ED3.3): close this editor — the pane, or the full-screen view. */
    onQuit?: () => void;
    /** After every change of the open file or its state. */
    onState?: (state: OpenFileState | null) => void;
    /** Shown while no file is open. */
    empty?: Snippet;
    /** The floating panel (T8, T9): too small for the minimap and sticky scroll. */
    compact?: boolean;
    /**
     * Opens another file at a line — a definition elsewhere (ED6.2): a new tab
     * where the host has tabs. Without it the file opens in this editor.
     */
    onOpenFile?: (file: { root: string; rel: string }, line: number) => void;
    /**
     * Shows a language server's list of places (ED6.3: uses, several
     * implementations) — in the host's search list. Without it the editor
     * goes to the first place.
     */
    onShowLocations?: (list: LocationList) => void;
    /** The outline view and breadcrumbs (#49): the file's symbols and the cursor's line, while this editor is on screen. */
    onOutline?: (info: OutlineInfo) => void;
  }

  let {
    visible = true,
    notice = null,
    onOpenRequest,
    onQuit,
    onState,
    empty,
    compact = false,
    onOpenFile,
    onShowLocations,
    onOutline,
  }: Props = $props();

  /** A file panel's rendered preview follows its text-size control; elsewhere the preview keeps its own size. */
  const previewFontSize = $derived(
    compact ? `calc(${$editorSettings.panelFontSize}px * var(--ax-ui-scale))` : undefined,
  );

  /** Quiet time after the last change before unsaved text is kept aside (F8). */
  const RECOVERY_DELAY_MS = 2000;
  /** How long the "reloaded" hint stays. */
  const HINT_MS = 3000;
  /** Quiet time before "autosave after a pause" saves (F9). */
  const AUTOSAVE_DELAY_MS = 1000;

  /** What is open: a text session, a picture, or nothing — one value, so the two can never both be set. */
  type Opened = { kind: "session"; session: FileSession } | { kind: "image"; root: string; rel: string } | null;
  let opened = $state.raw<Opened>(null);
  const session = $derived(opened?.kind === "session" ? opened.session : null);
  /** A raster image open instead of a text session (W3). */
  const image = $derived(opened?.kind === "image" ? opened : null);
  /** Bumped whenever the session's state changed; the banner and status derive from it. */
  let sessionTick = $state(0);
  /**
   * The surface's `revision`: bumped when it must redraw for a reason it cannot
   * see — the document changed outside it (reload, restore), or an injected
   * grammar finished loading and the colours changed.
   */
  let outside = $state(0);
  let wrap = $state(false);
  /** "Compare" (F10, H7): the file on disk against the text being edited, as a diff. */
  let compare = $state.raw<{ model: DiffModel; disk: string; mine: string } | null>(null);
  let surface = $state<EditorSurface | null>(null);
  /** Vi's mode pill (V8); `null` in the normal key map. */
  let viStatus = $state<ViStatus | null>(null);
  /** `:set nu`/`:set rnu` for this editor (V6); `null` follows the settings. */
  let numbersOverride = $state<GutterMode | null>(null);
  /** `:set list`: tabs and trailing spaces shown. */
  let list = $state(false);
  /** Syntax colours for the open file (ED2); `null` for plain text or a large file. */
  let highlighter = $state.raw<SyntaxHighlighter | null>(null);
  /** The open file on its language server (ED6), if it has one. */
  let lspDoc = $state.raw<LspDocument | null>(null);
  let stopDiagnostics: (() => void) | null = null;
  /** What the language server last reported about the open file (L6). */
  let diagnostics = $state.raw<DiagnosticSet | null>(null);
  /** A file with a rendered view (G8, W3): the source, the rendered view, or both side by side. */
  let viewMode = $state<ViewMode>("source");
  /** What the open file renders as, if anything. */
  let previewKind = $state<PreviewKind | null>(null);
  let preview = $state<MarkdownPreview | null>(null);
  /** A new note is being filed (`create_note` asks the agent where it goes). */
  let filing = $state(false);
  /** Source and preview scroll in step; whichever is scrolled leads (H12). */
  const scrollLink = new ScrollLink();
  let recoveryTimer: ReturnType<typeof setTimeout> | null = null;
  let autosaveTimer: ReturnType<typeof setTimeout> | null = null;
  let hintTimer: ReturnType<typeof setTimeout> | null = null;
  /** The font face the surfaces draw with — switched only once it has loaded. */
  const face = editorFace();

  /** Debug (#51): a Python file of a project takes breakpoints; they are kept one-based, the surface counts from zero. */
  const debuggable = $derived(session !== null && session.root.startsWith("project:") && isDebuggable(session.rel));
  const breakpointLines = $derived.by(() => {
    if (!debuggable || !session) return null;
    return new Set([...linesOf($breakpoints, session.root, session.rel)].map((l) => l - 1));
  });
  // Breakpoints (and their conditions) stay on their code while the text above them changes.
  $effect(() => {
    if (!debuggable || !session) return;
    const { root, rel, doc } = session;
    let count = doc.store.lineCount();
    return doc.onTextChange((change) => {
      const before = count;
      count = doc.store.lineCount();
      if ((get(breakpoints)[root]?.[rel] ?? []).length > 0) moveDebugBreakpoints(root, rel, change, before);
    });
  });
  /** Breakpoints with extras (zero-based), drawn paler; a right-click on a number edits them. */
  const conditionalLines = $derived.by(() => {
    if (!debuggable || !session) return null;
    const extras = $breakpointInfo[session.root]?.[session.rel] ?? {};
    return new Set(Object.keys(extras).map((l) => Number(l) - 1));
  });
  let bpEdit = $state<{ line: number; x: number; y: number } | null>(null);
  const execLine = $derived($execPoint && session && $execPoint.root === session.root && $execPoint.rel === session.rel ? $execPoint.line : null);

  const settings = $derived({
    ...surfaceSettings($editorSettings, wrap),
    // The floating window has its own text size (⌘+ ⌘- ⌘0); `rowH` and the font spec follow from it in the surface.
    ...(compact ? { fontSize: $editorSettings.panelFontSize } : {}),
    fontFamily: face.family,
    fontWeight: face.weight,
    lineNumbers: numbersOverride ?? $editorSettings.lineNumbers,
    list,
    // Not in the panel, and not in the light mode (T2).
    minimap: $editorSettings.minimap && !compact && !session?.light,
    stickyScroll: $editorSettings.stickyScroll && !compact && !session?.light,
  });

  /** Indentation for a file that shows none (F11). */
  function indentFallback(): Indent {
    const s = $editorSettings;
    return { kind: s.indentKind, size: s.indentKind === "tabs" ? s.tabSize : s.indentSize };
  }

  const banner = $derived.by(() => {
    void sessionTick;
    return session?.banner ?? null;
  });
  const status = $derived.by(() => {
    void sessionTick;
    return session ? statusParts(session.doc, settings.tabSize) : null;
  });

  // ------------------------------------------------------------ outline (#49)

  /** Quiet time after the last change before the symbols are worked out again. */
  const OUTLINE_DELAY_MS = 250;
  let outlineCache: { session: FileSession; revision: number; tree: unknown; symbols: OutlineSymbol[] | null } | null =
    null;
  let outlineTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    void sessionTick;
    const s = session;
    const hl = highlighter;
    if (!onOutline || !s || !visible) return;
    const line = s.doc.selection.head.line;
    untrack(() => {
      const revision = s.doc.revision;
      const tree = hl?.syntaxTree?.() ?? null;
      const c = outlineCache;
      const fresh = c && c.session === s && c.revision === revision && c.tree === tree;
      const now = () => {
        outlineCache = {
          session: s,
          revision: s.doc.revision,
          tree: hl?.syntaxTree?.() ?? null,
          symbols: s.light ? null : symbolsOf(s.doc.store, s.fileName, hl?.syntaxTree?.() ?? null),
        };
        onOutline({ symbols: outlineCache.symbols, line: s.doc.selection.head.line });
      };
      clearTimeout(outlineTimer);
      if (fresh) {
        onOutline({ symbols: c.symbols, line });
      } else if (!c || c.session !== s) {
        now();
      } else {
        // The cursor moves at once; the symbols catch up a moment after the last change.
        onOutline({ symbols: c.symbols, line });
        outlineTimer = setTimeout(now, OUTLINE_DELAY_MS);
      }
    });
    return () => clearTimeout(outlineTimer);
  });

  /** This editor's own id, for the shared list of files with unsaved changes. */
  const editorId = crypto.randomUUID();

  $effect(() => {
    void sessionTick;
    const s = session;
    if (s) onState?.({ root: s.root, rel: s.rel, dirty: s.doc.dirty, untitled: s.untitled });
    else onState?.(image ? { ...image, dirty: false, untitled: false } : null);
    markDirty(editorId, s && !s.untitled && s.doc.dirty ? { root: s.root, rel: s.rel } : null);
  });

  /** How long an agent's trail stays: the lines cool off sooner (the surface's CSS), the flag lingers. */
  const TRAIL_MS = 8000;
  let trail = $state<EditorTrail | null>(null);
  let trailTimer: ReturnType<typeof setTimeout> | null = null;

  /** What the last silent reload brought in, as the surface's trail — with the agent's flag when it is known whose. */
  function showTrail(s: FileSession): void {
    const t = s.trail;
    if (s !== session || !t || t.lines.length === 0) return;
    const statuses = get(agentStatus.statuses);
    const author = trailAuthor(s.root, get(projectSession).agents, (id) => statuses.byAgent.get(id)?.state === "working");
    trail = {
      lines: new Set(t.lines),
      at: t.at,
      ms: TRAIL_MS,
      flag:
        author && t.last !== null
          ? {
              line: t.last,
              text: harnessBadge(author.harness),
              ring: `var(--ax-harness-${author.harness})`,
              name: author.name,
              label: harnessLabel(author.harness),
            }
          : null,
    };
    if (trailTimer) clearTimeout(trailTimer);
    trailTimer = setTimeout(() => (trail = null), TRAIL_MS);
  }

  // Another file shows: the last one's trail goes with it.
  $effect(() => {
    void session;
    trail = null;
  });
  $effect(() => () => {
    if (trailTimer) clearTimeout(trailTimer);
  });

  /** After a session action or an external change — both may replace the text. */
  function refresh(): void {
    sessionTick++;
    outside++;
    if (session?.banner?.kind === "reloaded") {
      if (hintTimer) clearTimeout(hintTimer);
      hintTimer = setTimeout(() => {
        if (session?.banner?.kind === "reloaded") session.dismissBanner();
        sessionTick++;
      }, HINT_MS);
    }
  }

  /**
   * Opens `file` in place of the one open now — which is kept aside and left
   * only once the new one has opened — and puts the cursor on `line`. `intent`
   * "read" opens Markdown and HTML on their rendered view (W1).
   */
  export async function open(
    file: { root: string; rel: string },
    line: number | null = null,
    intent: OpenIntent = "edit",
  ): Promise<OpenResult> {
    if (isImagePath(file.rel)) {
      await leaveCurrent();
      opened = { kind: "image", root: file.root, rel: file.rel };
      refresh();
      return { ok: true };
    }
    if (session && session.root === file.root && session.rel === file.rel) {
      if (line !== null) goToLine(line);
      return { ok: true };
    }
    let next: FileSession;
    try {
      next = await FileSession.open(fileBackend, file.root, file.rel, indentFallback());
    } catch (err) {
      return { ok: false, kind: (err as { kind?: string }).kind ?? null, message: messageOf(err) };
    }
    await adopt(next, intent, line);
    return { ok: true };
  }

  /** A new note (W4): an untitled Markdown draft, filed by ⌘S. Only one at a time. */
  export async function newNote(): Promise<void> {
    if (session?.untitled) {
      surface?.focus();
      return;
    }
    await adopt(await FileSession.untitled(fileBackend, indentFallback()), "edit", null);
  }

  /** Makes `next` the open session, leaving the one before. */
  async function adopt(next: FileSession, intent: OpenIntent, line: number | null): Promise<void> {
    await leaveCurrent();
    // Folds kept from last time (T7) — unless the session brings its own (a hand-over).
    if (!next.untitled && next.folds.closed.length === 0) {
      next.folds.restore(rememberedFolds(foldKey(next.root, next.rel)), next.doc.store.lineCount());
    }
    opened = { kind: "session", session: next };
    wrap = wrapsByDefault($editorSettings, next.fileName);
    // Vi's `:set` holds for the file it was typed in, as the wrap toggle does (V6).
    numbersOverride = null;
    list = false;
    previewKind = previewKindFor(next.fileName);
    viewMode = initialViewMode(previewKind, intent);
    compare = null;
    refresh();
    void attachHighlighter(next);
    void attachLsp(next);
    await nextTick();
    if (line !== null) goToLine(line);
    else surface?.focus();
    // Only the page shows: it takes the keys (⌃- back) the hidden source cannot.
    if (viewMode === "preview") preview?.focus();
  }

  /**
   * Hands the open session to another editor (W11: from the panel to a tab)
   * without leaving it — nothing is saved, kept aside or unwatched, the undo
   * history goes along. This editor is empty afterwards. What moves is the text
   * and the rendered/source view (W11); this editor's own view state — an open
   * Compare, `:set` options, the wrap toggle — starts afresh in the new one.
   */
  export function detach(): { session: FileSession; viewMode: ViewMode } | null {
    const s = session;
    if (!s) return null;
    keepFolds(s);
    highlighter?.dispose();
    highlighter = null;
    detachLsp();
    if (recoveryTimer) clearTimeout(recoveryTimer);
    recoveryTimer = null;
    if (autosaveTimer) clearTimeout(autosaveTimer);
    autosaveTimer = null;
    opened = null;
    refresh();
    return { session: s, viewMode };
  }

  /** Takes over a session another editor let go of (`detach`), on the view it had there. */
  export async function adoptSession(handed: { session: FileSession; viewMode: ViewMode }): Promise<void> {
    await adopt(handed.session, "edit", null);
    if (previewKind) viewMode = handed.viewMode;
  }

  /**
   * Whether only a rendered view or a picture is on screen, so no editing key map (Vi's Normal mode, a completion)
   * can need Escape — the panel may then take it to close itself.
   */
  export function isReadOnlyView(): boolean {
    if (image) return true;
    return !!session && previewKind !== null && viewMode === "preview";
  }

  /** Whether closing now would leave unsaved text (or an unfiled note) behind. */
  export function hasUnsaved(): boolean {
    return !!session && session.doc.dirty;
  }

  /** Saves (or files the note); whether nothing unsaved is left. */
  export async function saveNow(): Promise<boolean> {
    await save();
    return !hasUnsaved();
  }

  /** Closing without saving: the kept-aside copy goes too, nothing is offered back next time. */
  export async function discard(): Promise<void> {
    const s = session;
    if (!s) return;
    s.doc.markSaved();
    await s.persistRecovery();
    sessionTick++;
  }

  /** Puts the cursor on `line` (zero-based) and focuses the text. */
  export function goToLine(line: number): void {
    surface?.goToLine(line);
    surface?.focus();
  }

  export function focus(): void {
    if (previewKind !== null && viewMode === "preview") preview?.focus();
    else surface?.focus();
  }

  /**
   * Starts highlighting `s` once its grammar has loaded (G4): nothing for plain
   * text, and nothing for a file in the light mode (T2). An injected language that
   * arrives later redraws through `outside`.
   */
  async function attachHighlighter(s: FileSession): Promise<void> {
    if (s.light) return;
    const stale = () => session !== s;
    const created = await highlightFor(s.doc, { fileName: s.fileName, onColours: () => outside++, stale });
    if (created) highlighter = created;
  }

  /**
   * Opens `s` on its language server (ED6), and follows what the server reports
   * about it. The full-screen app and the IDE only (L3) — not the floating
   * panel, not a file in the light mode, not a new note.
   */
  async function attachLsp(s: FileSession): Promise<void> {
    if (compact || s.light || s.untitled) return;
    const language = detectLanguage(s.fileName, s.doc.store.line(0));
    const found = await openOnServer(s.root, s.rel, language, s.doc);
    if (!found) return;
    if (session !== s || lspDoc) {
      found.close();
      return;
    }
    lspDoc = found;
    const { client } = found.connection;
    const show = () => {
      const store = s.doc.store;
      diagnostics = new DiagnosticSet(client.diagnosticsFor(found.uri), (line) =>
        line < store.lineCount() ? store.line(line).length : 0,
      );
    };
    stopDiagnostics = client.onDiagnostics((uri) => {
      if (uri === found.uri) show();
    });
    show();
  }

  /** Lets go of the language server for the file being left. */
  function detachLsp(): void {
    stopDiagnostics?.();
    stopDiagnostics = null;
    lspDoc?.close();
    lspDoc = null;
    diagnostics = null;
  }

  /** F8 / ⇧F8 and Vi's `]d` / `[d`: the next or previous problem (L7). */
  function goToProblem(dir: 1 | -1): boolean {
    const s = session;
    if (!s || !diagnostics || diagnostics.size === 0) return false;
    const here = s.doc.selection.head;
    const found = dir === 1 ? diagnostics.next(here) : diagnostics.previous(here);
    if (found) surface?.goTo(found.start);
    return true;
  }

  function interceptKey(e: KeyboardEvent): boolean {
    // ⌘. always, ⌘L where a server offers code actions — else ⌘L selects the line (L21).
    const c = chord(e, $platform);
    const cmdOnly = c.primary && !c.alt && !c.shift;
    const actionsHere = !!lspDoc?.connection.client.codeActions;
    if (cmdOnly && (e.key === "." || (e.key === "l" && actionsHere))) {
      e.preventDefault();
      if (lspDoc) void surface?.openCodeActions();
      else toast(tEditor("noLanguageServer"), "info");
      return true;
    }
    if (e.key === "F2" && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey && lspDoc) {
      e.preventDefault();
      void startRename(null);
      return true;
    }
    // ⇧⌥F: format (L7) — `code`, since ⌥ turns the key into another character.
    if (e.code === "KeyF" && e.shiftKey && e.altKey && !e.metaKey && !e.ctrlKey) {
      e.preventDefault();
      void formatNow();
      return true;
    }
    if (e.key === "F12" && !foreignModifier(e, $platform) && !e.altKey && lspDoc) {
      // F12 definition, ⌘F12 implementation, ⇧F12 uses (L7, ED6.3).
      const kind: LocationKind = c.primary ? "implementation" : e.shiftKey ? "references" : "definition";
      if (c.primary && e.shiftKey) return false;
      e.preventDefault();
      void goToLocations(kind, session?.doc.selection.head ?? null);
      return true;
    }
    if (e.metaKey || e.ctrlKey || e.altKey) return false;
    if (e.key !== "F8" || !goToProblem(e.shiftKey ? -1 : 1)) return false;
    e.preventDefault();
    return true;
  }

  /**
   * The open file's completion (ED6.4), for the surface's menu: asked of its
   * server once the server is up — one that does not complete offers nothing.
   */
  const serverCompletion = $derived.by((): CompletionPort | null => {
    const found = lspDoc;
    if (!found) return null;
    const { client } = found.connection;
    return {
      ask: async (at, trigger, again) =>
        (await client.whenReady()) && client.completes
          ? client.completion(found.uri, at, trigger, again)
          : { items: [], incomplete: false },
      resolve: (item) => client.resolveCompletion(item),
      triggers: () => client.completionTriggers,
    };
  });

  /** In a vault note, notes after `[[` and tags after `#` beside the server's completion (TL2, Q4). */
  const completionPort = $derived.by((): CompletionPort | null => {
    const s = session;
    const from = linkFrom;
    if (!s || from === null) return serverCompletion;
    return noteCompletion(serverCompletion, s.doc, from, { notes: loadNoteIndex, tagList: loadTagList });
  });

  /** The open file's signature help (ED6.6), for the surface's hint. */
  const signaturePort = $derived.by((): SignaturePort | null => {
    const found = lspDoc;
    if (!found) return null;
    const { client } = found.connection;
    return {
      ask: async (at, trigger, retrigger) =>
        (await client.whenReady()) ? client.signatureHelp(found.uri, at, trigger, retrigger) : null,
      triggers: () => client.signatureTriggers,
      retriggers: () => client.signatureRetriggers,
    };
  });

  /**
   * The open file's code actions (ED6.7), for the surface's menu: the
   * problems in the range go along as the request's context; the hover's
   * "Fix…" (`fix`) asks about its one problem and keeps only its actions.
   */
  const codeActionPort = $derived.by((): CodeActionPort | null => {
    const found = lspDoc;
    if (!found) return null;
    const { client } = found.connection;
    return {
      ask: async (from, to, fix) => {
        if (!(await client.whenReady()) || !client.codeActions) return [];
        const problems = fix ? [fix] : problemsIn(from, to);
        const context = problems.map((d) => d.raw).filter((raw) => raw !== undefined);
        const items = await client.codeActionsAt(found.uri, from, to, context);
        return fix ? fixesOnly(items) : items;
      },
      take: (item) => void takeCodeAction(item),
      none: () =>
        toast(client.codeActions ? tEditor("noCodeActionsHere") : tEditor("serverNoCodeActions"), "info"),
    };
  });

  /** The problems the server reported in `from`..`to` (at the cursor: the ones under it). */
  function problemsIn(from: { line: number; col: number }, to: { line: number; col: number }): Diagnostic[] {
    const set = diagnostics;
    if (!set) return [];
    if (from.line === to.line && from.col === to.col) return set.at(from);
    const before = (a: { line: number; col: number }, b: { line: number; col: number }) =>
      a.line < b.line || (a.line === b.line && a.col <= b.col);
    return set.all.filter((d) => before(d.start, to) && before(from, d.end));
  }

  /**
   * Carries out a taken code action (ED6.7, L18, L23): filled in first when
   * it came without its edit, then its edit applied (as a rename's, L16), then
   * its command run — an edit the server sends back for that command is
   * applied the same way. Text typed meanwhile wins: nothing is done.
   */
  async function takeCodeAction(item: CodeActionItem): Promise<void> {
    const s = session;
    const found = lspDoc;
    if (!s || !found) return;
    if (s.readOnly) return void toast(tEditor("fileReadOnly"), "info");
    const { client } = found.connection;
    const revision = s.doc.revision;
    let action: CodeActionItem;
    try {
      action = await client.resolveCodeAction(item);
    } catch (err) {
      return void toast(tEditor("actionCouldNotRun", { title: item.title, error: messageOf(err) }), "danger");
    }
    if (session !== s) return;
    if (s.doc.revision !== revision) {
      return void toast(tEditor("actionTextChanged"), "info");
    }
    if (action.edit === null && !action.command) {
      return void toast(tEditor("actionChangesNothing", { title: action.title }), "info");
    }
    if (action.edit !== null && !(await applyServerEdit(s, found, action.edit, action.title))) return;
    if (!action.command) return;
    let before = s.doc.revision;
    try {
      await client.executeCommand(action.command, async (edit) => {
        // Only onto the text the command was run on: typing meanwhile makes the server's edit wrong.
        if (session !== s || s.doc.revision !== before) return false;
        const applied = await applyServerEdit(s, found, edit, action.title);
        before = s.doc.revision;
        return applied;
      });
    } catch (err) {
      toast(tEditor("actionFailed", { title: action.title, error: messageOf(err) }), "danger");
    }
  }

  /**
   * Applies a server's `WorkspaceEdit` for the action `title` where each file
   * is (L16/L23); whether all of it went in. One that would move files is refused.
   */
  async function applyServerEdit(s: FileSession, found: LspDocument, raw: unknown, title: string): Promise<boolean> {
    let edits;
    try {
      edits = parseWorkspaceEdit(raw);
    } catch {
      toast(tEditor("actionMovesFiles", { title }), "danger");
      return false;
    }
    const outcome = await applyWorkspaceEdit(edits, editPorts(s, found));
    if (outcome.refused) {
      toast(tEditor("actionNotApplied", { title, reason: outcome.refused }), "danger");
      return false;
    }
    if (outcome.skipped.length > 0) {
      const list = outcome.skipped.map((k) => `${k.file} (${k.reason})`).join(", ");
      toast(tEditor("actionNotChanged", { title, list }), "danger");
    }
    return outcome.skipped.length === 0;
  }

  /** Where a server's edit goes (L16): this editor through its surface, other open editors, closed files. */
  function editPorts(s: FileSession, found: LspDocument): EditPorts {
    const { client } = found.connection;
    return {
      here: { uri: found.uri, doc: s.doc, apply: (changes) => surface?.applyCommand({ type: "replaceText", changes }) },
      openDoc: (uri) => client.documentFor(uri),
      fileOf: (uri) => {
        const file = definitionFile(s.root, found, { uri, at: { line: 0, col: 0 } });
        return file && !file.root.startsWith(FOREIGN_ROOT) ? file : null;
      },
      read: (root, rel) => fileBackend.read(root, rel),
      write: (root, rel, content, expected) => fileBackend.write(root, rel, content, expected),
    };
  }

  /** What the language server says about the symbol at `at` (the surface's hover, Vi's `K`). */
  function hoverAt(at: { line: number; col: number }): Promise<string | null> {
    const found = lspDoc;
    return found ? found.connection.client.hover(found.uri, at) : Promise.resolve(null);
  }

  /** What each kind of place is called, for the list's title (`{symbol}`) and "none found" — catalog keys. */
  const LOCATION_WORDS: Record<LocationKind, { title: EditorText; none: EditorText }> = {
    definition: { title: "definitionsOf", none: "noDefinition" },
    implementation: { title: "implementationsOf", none: "noImplementation" },
    typeDefinition: { title: "typeDefinitionsOf", none: "noTypeDefinition" },
    references: { title: "referencesTo", none: "noReferences" },
  };

  /**
   * F12, ⌘-click and Vi's `gd` (ED6.2, L7): the definition of the symbol at
   * `at` — here, in another file of the root (a tab where the host has them),
   * or read-only outside every root (L11).
   */
  function goToDefinition(at: { line: number; col: number } | null): Promise<void> {
    return goToLocations("definition", at);
  }

  /**
   * The places of `kind` for the symbol at `at` (ED6.2/ED6.3): one is gone to
   * as a definition is; several are listed (`onShowLocations`), or the first
   * is gone to where the host has no list. Uses are always listed.
   */
  async function goToLocations(kind: LocationKind, at: { line: number; col: number } | null): Promise<void> {
    const s = session;
    const found = lspDoc;
    if (!s || !found || !at) return;
    const places = await found.connection.client.locations(kind, found.uri, at);
    if (session !== s) return;
    const words = LOCATION_WORDS[kind];
    if (places.length === 0) {
      toast(tEditor(words.none), "info");
      return;
    }
    if (onShowLocations && (places.length > 1 || kind === "references")) {
      const title = tEditor(words.title, { symbol: symbolAt(s, at) ?? tEditor("theSymbol") });
      const list = await buildLocationList(
        title,
        places,
        (location) => definitionFile(s.root, found, location),
        async (root, rel) => (await fileBackend.read(root, rel)).content,
      );
      if (session === s) onShowLocations(list);
      return;
    }
    goToPlace(s, found, places[0]);
  }

  /** Goes to one place: here, in another file of the root, or read-only outside every root. */
  function goToPlace(s: FileSession, found: LspDocument, location: Location): void {
    const file = definitionFile(s.root, found, location);
    if (!file) {
      toast(tEditor("placeNotFile"), "info");
      return;
    }
    if (file.root === s.root && file.rel === s.rel) {
      surface?.goTo(location.at);
      return;
    }
    if (!s.untitled) jumpHistory.record(placeOf(s));
    if (onOpenFile) onOpenFile(file, location.at.line);
    else void open(file, location.at.line);
  }

  // ------------------------------------------------------------ wikilinks and tags (TL2)

  /**
   * Where the open file's root lies in the vault (`noteIndex.ts`, `vaultPrefix`): `""` for the workspace or a project
   * opened on the vault's folder, a folder inside it for one opened deeper; `null` outside the vault. Asked once per
   * opened file.
   */
  let vaultAt = $state.raw<{ session: FileSession; prefix: string } | null>(null);
  $effect(() => {
    const s = session;
    if (!s || s.untitled || previewKind !== "markdown") return;
    let stale = false;
    void vaultPrefix(s.root).then((prefix) => {
      if (!stale && session === s) vaultAt = prefix === null ? null : { session: s, prefix };
    });
    return () => {
      stale = true;
    };
  });
  /** The open file's place in the vault: its prefix there, if it is in the vault. */
  const vaultPlace = $derived.by((): string | null => {
    const s = session;
    if (!s) return null;
    if (s.root === "workspace" || s.untitled) return "";
    return vaultAt?.session === s ? vaultAt.prefix : null;
  });

  /**
   * Where this note's links start from: its path in the vault, `""` for a new note not filed yet (it will be a vault
   * note); `null` for anything that is no Markdown of the vault — a code project's README has no wikilinks to follow.
   */
  const linkFrom = $derived.by((): string | null => {
    const s = session;
    const prefix = vaultPlace;
    if (!s || previewKind !== "markdown" || prefix === null) return null;
    return s.untitled ? "" : vaultRel(prefix, s.rel);
  });

  /** A note of the vault as a file to open: under this file's own root when it lies there (a vault project's tab). */
  function vaultFile(path: string): { root: string; rel: string } {
    const s = session;
    const prefix = vaultPlace;
    if (s && !s.untitled && s.root !== "workspace" && prefix !== null) {
      const rel = rootRel(prefix, path);
      if (rel !== null) return { root: s.root, rel };
    }
    return { root: "workspace", rel: path };
  }

  /** Bumped when the vault's notes were read again: dangling links are drawn anew. */
  let notesTick = $state(0);

  /** Whether the link `target` from the note `from` finds a note (`null` while the vault is not known yet). */
  function linkExists(from: string, target: string): boolean | null {
    if (!target) return true;
    const index = peekNoteIndex();
    return index ? index.resolve(from, target) !== null : null;
  }

  /** The surface's colours: in a note of the vault its links and tags over the grammar's. */
  const shownHighlighter = $derived.by(() => {
    const s = session;
    const from = linkFrom;
    if (!s || from === null || s.light) return highlighter;
    return withNoteMarks(highlighter, s.doc, (m) => linkExists(from, m.target));
  });

  /** The reading view's links, for a vault note; a new object once the vault is known, so it renders again. */
  const previewNotes = $derived.by((): NoteLinkRendering | null => {
    void notesTick;
    const from = linkFrom;
    return from === null ? null : { exists: (target) => linkExists(from, target) };
  });

  /** Where the cursor of `s` is, as a place of the jump history. */
  function placeOf(s: FileSession): JumpPlace {
    const head = s.doc.selection.head;
    return { root: s.root, rel: s.rel, line: head.line, col: head.col };
  }

  /** Another file at `line`: a tab where the host has them, else here (on its reading view from a reading view). */
  function goToFile(file: { root: string; rel: string }, line: number | null, intent: OpenIntent = "edit"): void {
    if (onOpenFile) onOpenFile(file, line ?? 0);
    else void open(file, line, intent);
  }

  /** ⌃- / ⌃⇧- and Vi's `<C-o>`/`<C-i>` past this document's list: along the app-wide history; whether it moved. */
  function walkHistory(dir: "back" | "forward"): boolean {
    const s = session;
    const place = dir === "back" ? jumpHistory.back(s && !s.untitled ? placeOf(s) : null) : jumpHistory.forward();
    if (!place) return false;
    if (s && place.root === s.root && place.rel === s.rel) {
      surface?.goTo({ line: place.line, col: place.col });
      if (viewMode !== "source") preview?.scrollToLine(place.line);
    } else {
      goToFile({ root: place.root, rel: place.rel }, place.line, viewMode === "preview" ? "read" : "edit");
    }
    return true;
  }

  /**
   * ⌘-click and Vi's `gd` on a note's link or tag (TL2): follows it; whether there was one at `at`. Never in code
   * (a fenced block, inline code, the frontmatter).
   */
  function followMarkAt(at: { line: number; col: number }): boolean {
    const s = session;
    // Outside the vault (a code project's README) a `#word` or `[[x]]` is no note's: the language server answers.
    if (!s || linkFrom === null) return false;
    const mark: NoteMark | null = noteMarkAt(s.doc, at);
    if (!mark) return false;
    if (mark.kind === "tag") showTag(mark.tag);
    else void followLink(mark.target, mark.heading, "edit");
    return true;
  }

  /** A tag was followed: the Second Brain, searching for it (its tag filter is TL3's). */
  function showTag(tag: string): void {
    emit("open-second-brain", { focus: null, query: `#${tag}` });
  }

  /**
   * Follows a wikilink to `target` (at `heading`): the note it finds, a heading of this note (`[[#Heading]]`), or — a
   * link into nothing — a new note beside this one, titled after the target (Q3). The place it left goes into the
   * jump history first.
   */
  async function followLink(target: string, heading: string | null, intent: OpenIntent): Promise<void> {
    const s = session;
    const from = linkFrom;
    if (!s) return;
    // Where the jump starts, before anything is awaited: the cursor may move meanwhile.
    const here = placeOf(s);
    if (from === null) return void toast(tEditor("wikilinksWorkspaceOnly"), "info");
    if (!target) {
      const line = heading ? headingLine(s.doc.store.text(), heading) : null;
      if (line === null) return void toast(tEditor("noHeading", { heading: heading ?? "" }), "info");
      if (!s.untitled) jumpHistory.record(here);
      surface?.goTo({ line, col: 0 });
      if (viewMode !== "source") preview?.scrollToLine(line);
      return;
    }
    let index = await loadNoteIndex();
    // A read that failed a while ago is not trusted for a click: the owner asks now, so ask the vault again.
    if (!index) {
      invalidateNoteIndex();
      index = await loadNoteIndex();
    }
    if (session !== s) return;
    if (!index) return void toast(tEditor("vaultUnreadable"), "danger");
    const rel = index.resolve(from, target);
    if (rel === null) {
      const several = index.sameStem(target);
      if (several.length > 1) {
        const names = several.map((p) => p.replace(/\.md$/i, "")).join(", ");
        return void toast(tEditor("severalNotes", { target, names }), "info");
      }
      return void createLinkedNote(s, from, target, here);
    }
    const file = vaultFile(rel);
    let line: number | null = null;
    if (heading) {
      try {
        line = headingLine((await fileBackend.read(file.root, file.rel)).content, heading);
      } catch {
        line = null;
      }
      if (session !== s) return;
    }
    if (!s.untitled) jumpHistory.record(here);
    if (file.root === s.root && file.rel === s.rel) {
      surface?.goTo({ line: line ?? 0, col: 0 });
      if (viewMode !== "source" && line !== null) preview?.scrollToLine(line);
      return;
    }
    goToFile(file, line, intent);
  }

  /** A link into nothing (Q3): its note, created beside `from` with the target as its title, then opened. */
  async function createLinkedNote(s: FileSession, from: string, target: string, here: JumpPlace): Promise<void> {
    if (s.untitled) return void toast(keyHints(tEditor("fileNoteFirst"), $platform), "info");
    const path = newNotePath(from, target);
    if (!path) return void toast(tEditor("unusableFileName", { target }), "info");
    const file = vaultFile(path);
    try {
      // `file_create` refuses to replace anything (`O_EXCL`); the title goes in against the version it made.
      const version = await createFile(file.root, file.rel);
      await fileBackend.write(file.root, file.rel, `# ${fileStem(path)}\n\n`, version);
    } catch (err) {
      return void toast(tEditor("couldNotCreate", { path, error: messageOf(err) }), "danger");
    }
    invalidateNoteIndex();
    void loadNoteIndex();
    if (session !== s) return;
    jumpHistory.record(here);
    toast(tEditor("created", { path }), "info");
    // Below the title, ready to write — in the source, since a new note has nothing to read yet.
    goToFile(file, 2, "edit");
  }

  /** A click on a wikilink in the reading view (its inner text `Target#Heading|alias`). */
  function onPreviewLink(inner: string): void {
    const { target, heading } = parseWikilink(inner);
    void followLink(target, heading, "read");
  }

  /** ⌘-click: a note's link or tag first, else the language server's definition; `false` lets the click through. */
  function definitionAt(at: { line: number; col: number }): boolean {
    if (followMarkAt(at)) return true;
    if (!lspDoc) return false;
    void goToDefinition(at);
    return true;
  }

  /** The word at `at` in quotes (the list's title), `null` on no word. */
  function symbolAt(s: FileSession, at: { line: number; col: number }): string | null {
    if (at.line >= s.doc.store.lineCount()) return null;
    const line = s.doc.store.line(at.line);
    const word = /[\p{L}\p{N}_$]/u;
    let from = at.col;
    let to = at.col;
    while (from > 0 && word.test(line[from - 1])) from--;
    while (to < line.length && word.test(line[to])) to++;
    return to > from ? `\`${line.slice(from, to)}\`` : null;
  }

  /** The rename field (L17): where it shows, the name in it, and where the rename was asked. */
  let renameBox = $state<{ x: number; y: number; name: string; at: { line: number; col: number } } | null>(null);
  let renameInput = $state<HTMLInputElement | null>(null);

  /**
   * F2, Vi `grn` and `:rename {name}` (ED6.5, L16, L17): with a name the
   * symbol at the cursor is renamed at once; without, the field opens at the
   * cursor with the current name.
   */
  async function startRename(name: string | null): Promise<void> {
    const s = session;
    const found = lspDoc;
    if (!s || !found) return void toast(tEditor("noLanguageServer"), "info");
    const { client } = found.connection;
    if (!(await client.whenReady()) || !client.renames) {
      return void toast(tEditor("serverNoRename"), "info");
    }
    const at = s.doc.selection.head;
    const line = s.doc.store.line(at.line);
    const word = wordAt(line, at.col);
    const prepared = await client.prepareRename(found.uri, at, line.slice(word.start, word.end));
    if (session !== s) return;
    if (!prepared) return void toast(tEditor("nothingToRename"), "info");
    if (name !== null) return void rename(at, name);
    const point = surface?.cursorPoint() ?? { x: 0, y: 0 };
    renameBox = { ...point, name: prepared.name, at };
    await nextTick();
    renameInput?.select();
  }

  function onRenameKey(e: KeyboardEvent): void {
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      renameBox = null;
      surface?.focus();
    } else if (e.key === "Enter" && renameBox) {
      e.preventDefault();
      const { at, name } = renameBox;
      renameBox = null;
      surface?.focus();
      if (name.trim()) void rename(at, name.trim());
    }
  }

  /** Renames the symbol at `at` everywhere the server says (L16), and tells how it went. */
  async function rename(at: { line: number; col: number }, newName: string): Promise<void> {
    const s = session;
    const found = lspDoc;
    if (!s || !found) return;
    // The server's edits are for the text as it is now; typing while it works would make them wrong.
    const revision = s.doc.revision;
    let edits;
    try {
      edits = await found.connection.client.rename(found.uri, at, newName);
    } catch (err) {
      // A rename that would move files (a Rust module) is refused, as the client says it moves none (L16).
      const message = messageOf(err);
      const movesFiles = /rename capability|create, move or delete files/i.test(message);
      const text = movesFiles ? tEditor("renameMovesFiles") : tEditor("couldNotRename", { error: message });
      return void toast(text, "danger");
    }
    if (session !== s) return;
    if (s.doc.revision !== revision) {
      return void toast(tEditor("renameTextChanged"), "info");
    }
    const outcome = await applyWorkspaceEdit(edits, editPorts(s, found));
    if (outcome.refused) return void toast(tEditor("nothingRenamed", { reason: outcome.refused }), "danger");
    const files = tEditor("fileCount", { n: outcome.changed, count: formatNumber(outcome.changed) });
    if (outcome.skipped.length === 0) toast(tEditor("renamed", { name: newName, files }), "info");
    else {
      const list = outcome.skipped.map((k) => `${k.file} (${k.reason})`).join(", ");
      toast(tEditor("renamedPartly", { name: newName, files, list }), "danger");
    }
  }

  /** The folds of `s` as they are now (edits moved them), if they are kept at all. */
  function keepFolds(s: FileSession): void {
    if (!s.untitled) updateRememberedFolds(foldKey(s.root, s.rel), s.folds.serialize());
  }

  /** Folds were opened or closed: keep them for next time (T7). */
  function onFolds(): void {
    const s = session;
    if (s && !s.untitled) rememberFolds(foldKey(s.root, s.rel), s.folds.serialize());
  }

  /** Keeps unsaved text aside and stops watching the file being left. */
  async function leaveCurrent(): Promise<void> {
    highlighter?.dispose();
    highlighter = null;
    detachLsp();
    if (recoveryTimer) clearTimeout(recoveryTimer);
    recoveryTimer = null;
    if (!session) return;
    keepFolds(session);
    if ($editorSettings.autosave !== "off") await session.save();
    await session.persistRecovery();
    await session.close();
  }

  async function save(confirmed = false): Promise<void> {
    if (!session) return;
    if (session.untitled) return fileNote();
    await session.save(confirmed);
    refresh();
  }

  /** ⌘S and `:w`: formatted first where that is on (L14) — autosave never formats. */
  async function saveExplicitly(): Promise<void> {
    const s = session;
    if (s && !s.untitled && !s.readOnly && formatsOnSave($editorSettings, languageOf(s))) {
      await formatNow(true);
    }
    await save();
  }

  function languageOf(s: FileSession): string | null {
    return detectLanguage(s.fileName, s.doc.store.line(0));
  }

  /**
   * Formats the open file (ED6.5, L13): with its language's formatter (Rust
   * picks it), else the language server's formatting. The changes go in as one
   * step; text typed meanwhile wins (nothing is applied). On save a formatter
   * that fails leaves the file as it is and says why; the save goes on.
   */
  async function formatNow(onSave = false): Promise<void> {
    const s = session;
    if (!s || s.untitled || s.readOnly) return;
    const language = languageOf(s);
    const input = s.doc.textForSave();
    const revision = s.doc.revision;
    let output: string | null = null;
    let missing: string | null = null;
    try {
      const result = await invokeBackend<Formatted>("file_format", {
        root: s.root,
        rel: s.rel,
        language: language ?? "",
        text: input,
      });
      if (result.kind === "done") output = result.text;
      else if (result.kind === "failed") {
        toast(`${result.formatter}: ${result.message}`, "danger");
        return;
      } else if (result.kind === "missing") missing = result.formatter;
    } catch (err) {
      toast(tEditor("couldNotFormat", { error: messageOf(err) }), "danger");
      return;
    }
    if (output === null) {
      const found = lspDoc;
      const edits = found
        ? await found.connection.client.formatting(found.uri, {
            tabSize: s.doc.indent.kind === "tabs" ? $editorSettings.tabSize : s.doc.indent.size,
            insertSpaces: s.doc.indent.kind !== "tabs",
          })
        : null;
      if (!edits) {
        if (!onSave || missing) {
          toast(missing ? tEditor("notInstalled", { name: missing }) : tEditor("noFormatter"), "info");
        }
        return;
      }
      output = applyTextEdits(input.replace(/\r\n?/g, "\n"), edits);
    }
    if (session !== s || s.doc.revision !== revision) return;
    const changes = textChanges(s.doc.store.text(), storeBody(output));
    if (changes.length > 0) surface?.applyCommand({ type: "replaceText", changes });
  }

  /**
   * Files the new note (W4, D19): the agent picks the area and the name, the
   * draft's kept-aside copy goes, and the editor opens the note it became.
   */
  async function fileNote(): Promise<void> {
    const draft = session;
    if (!draft?.untitled || filing) return;
    const content = draft.doc.textForSave();
    if (!content.trim()) return;
    filing = true;
    try {
      const rel = await invokeBackend<string>("create_note", { content });
      draft.doc.markSaved();
      await fileBackend.recoveryDelete(DRAFT_ROOT, DRAFT_REL).catch(() => undefined);
      await open({ root: "workspace", rel }, null, "read");
    } catch (err) {
      draft.banner = { kind: "error", message: tEditor("couldNotFile", { error: messageOf(err) }) };
      refresh();
    } finally {
      filing = false;
    }
  }

  /** ⌘⇧V: source → rendered → side by side → source (G8, W3). */
  function cyclePreview(): void {
    if (!previewKind) return;
    viewMode = nextViewMode(viewMode);
    // The keys follow what shows: the page alone, or the source again.
    void nextTick().then(() => focus());
  }

  /** The source scrolled: the preview follows, unless this is the echo of it following. */
  function onSourceTopLine(line: number): void {
    if (scrollLink.scrolled("source")) preview?.scrollToLine(line);
  }

  function onPreviewTopLine(line: number): void {
    if (scrollLink.scrolled("preview")) surface?.scrollToLine(line);
  }

  const previewText = $derived.by(() => {
    void sessionTick;
    void outside;
    return previewKind && viewMode !== "source" && session ? session.doc.store.text() : "";
  });

  function onEffect(effect: Effect): void {
    if (effect === "togglePreview") cyclePreview();
    else if (effect === "save") void saveExplicitly();
    else if (effect === "open") onOpenRequest?.();
    else if (effect === "toggleWrap") wrap = !wrap;
  }

  /** What Vi asks of the editor around the surface: save, close, another file's mark. */
  function onViEffect(effect: ViEffect): void {
    if (effect.type === "save") void saveExplicitly();
    else if (effect.type === "saveQuit") void saveExplicitly().then(() => onQuit?.());
    else if (effect.type === "format") void formatNow();
    else if (effect.type === "rename") void startRename(effect.name);
    else if (effect.type === "codeAction") {
      if (!lspDoc) toast(tEditor("noLanguageServer"), "info");
      else void surface?.openCodeActions(effect.range?.start, effect.range?.end);
    }
    else if (effect.type === "quit") {
      // Unsaved text is kept aside either way (F8); only a forced quit leaves it unsaved.
      if (!effect.force && session?.doc.dirty) return;
      onQuit?.();
    } else if (effect.type === "fileMark") {
      const [root, rel] = effect.file.split("\0");
      if (root && rel && session && !session.untitled) jumpHistory.record(placeOf(session));
      if (root && rel) void open({ root, rel }, effect.at.line);
    } else if (effect.type === "problem") {
      goToProblem(effect.dir);
    } else if (effect.type === "definition" || effect.type === "hover" || effect.type === "locations") {
      // Vi's keys exist for every file; say why nothing happens where no server runs.
      const head = session?.doc.selection.head ?? null;
      // `gd` on a note's link follows it (TL2), before any server is asked.
      if (effect.type === "definition" && head && followMarkAt(head)) return;
      if (!lspDoc) toast(tEditor("noLanguageServer"), "info");
      else if (effect.type === "definition") void goToDefinition(head);
      else if (effect.type === "locations") void goToLocations(effect.kind, head);
      else surface?.showHoverAtCursor();
    } else if (effect.type === "reload") {
      // `:e!` — the `!` is the confirmation the Reload button would ask for.
      void act((s) => s.discardChanges());
    } else if (effect.type === "set") {
      const next = applySet({ wrap, lineNumbers: settings.lineNumbers, list }, effect.option, effect.value);
      wrap = next.wrap;
      numbersOverride = next.lineNumbers;
      list = next.list;
    }
  }

  function onChange(): void {
    sessionTick++;
    if (recoveryTimer) clearTimeout(recoveryTimer);
    const current = session;
    recoveryTimer = setTimeout(() => void current?.persistRecovery(), RECOVERY_DELAY_MS);
    if ($editorSettings.autosave === "delay") {
      if (autosaveTimer) clearTimeout(autosaveTimer);
      autosaveTimer = setTimeout(() => {
        if (current === session && current?.doc.dirty) void save();
      }, AUTOSAVE_DELAY_MS);
    }
  }

  /** "Autosave when leaving": the editor is hidden or the window loses focus. */
  function saveOnLeave(): void {
    if ($editorSettings.autosave === "leave" && session?.doc.dirty) void save();
  }

  async function act(action: (s: FileSession) => Promise<void> | void): Promise<void> {
    if (!session) return;
    await action(session);
    refresh();
  }

  async function showCompare(): Promise<void> {
    if (!session) return;
    const disk = await session.diskText();
    const mine = session.doc.store.text();
    const hunks = hunksFromTexts(textLines(disk), textLines(mine));
    compare = { model: new DiffModel({ hunks, oldLines: textLines(disk), newLines: textLines(mine) }), disk, mine };
  }

  /** What the previews draw with in the floating window; `undefined` leaves them as they are (Studio, file app). */
  const previewZoomFactor = $derived(compact ? previewZoom($editorSettings.panelFontSize) : undefined);

  /**
   * ⌘S and ⌘⇧V for the whole editor, so they also work while focus is on a
   * banner button. Handled in the capture phase and stopped there: the
   * surface would otherwise see the same key and save a second time.
   * ⌘+ ⌘- ⌘0 resize the text of the floating window, and only there.
   */
  function onKeydownCapture(e: KeyboardEvent): void {
    // ⌃- / ⌃⇧- (Ctrl+Alt+← / → off the Mac): the app-wide jump history (TL2), from the source and the reading view.
    const walk = jumpKey(e, $platform);
    if (walk && (session || image)) {
      e.preventDefault();
      e.stopPropagation();
      if (!walkHistory(walk)) toast(walk === "back" ? tEditor("nothingBack") : tEditor("nothingForward"), "info");
      return;
    }
    const c = chord(e, $platform);
    if (!c.primary || c.alt || c.typing) return;
    const zoomStep = compact ? panelZoomStep(e, $platform) : null;
    if (zoomStep) {
      // `preventDefault` keeps the webview from zooming the whole app by itself.
      e.preventDefault();
      e.stopPropagation();
      updateEditorSettings({ panelFontSize: applyPanelZoomStep($editorSettings.panelFontSize, zoomStep) });
      return;
    }
    const key = c.key;
    const previewKey = key === "v" && c.shift && previewKind !== null;
    if (key !== "s" && !previewKey) return;
    e.preventDefault();
    e.stopPropagation();
    if (previewKey) cyclePreview();
    else void saveExplicitly();
  }

  $effect(() => {
    if (!visible) saveOnLeave();
  });

  onMount(() => {
    void ensureEditorSettingsLoaded();
    // The vault's notes were read again: links into nothing are drawn (and rendered) anew.
    const stopNotes = onNoteIndex(() => {
      notesTick++;
      outside++;
    });
    const unlisten = listenBackend<FileChange>("files:changed", (change) => {
      const s = session;
      void s?.onExternalChange(change).then((shown) => {
        if (!shown) return;
        showTrail(s);
        refresh();
      });
    });
    // A rename or delete made in the file app reaches every copy of the file this way (W13) —
    // before the watcher, which only sees the old path go, could call a rename a deletion.
    const unlistenRenamed = listenBackend<FileRenamed>("files:renamed", (renamed) => {
      const s = session;
      const rel = s && s.root === renamed.root ? renamedPath(s.rel, renamed.from, renamed.to) : null;
      if (s && rel !== null) {
        void s.moved(rel).then(() => {
          refresh();
          // A new name is a new document for the language server.
          detachLsp();
          void attachLsp(s);
        });
      }
    });
    const unlistenRemoved = listenBackend<FileRemoved>("files:removed", (removed) => {
      const s = session;
      if (!s || s.root !== removed.root || !isUnder(s.rel, removed.rel)) return;
      const gone = { root: s.root, rel: s.rel, kind: "deleted" as const, version: null };
      void s.onExternalChange(gone).then((shown) => shown && refresh());
    });
    window.addEventListener("blur", saveOnLeave);
    return () => {
      stopNotes();
      window.removeEventListener("blur", saveOnLeave);
      void unlisten.then((off) => off());
      void unlistenRenamed.then((off) => off());
      void unlistenRemoved.then((off) => off());
      if (hintTimer) clearTimeout(hintTimer);
      if (autosaveTimer) clearTimeout(autosaveTimer);
      detachLsp();
      // Going away is leaving the file: keep unsaved text aside, stop watching.
      markDirty(editorId, null);
      void leaveCurrent();
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="file-editor" onkeydowncapture={onKeydownCapture}>
  {#if notice}
    <div class="banner notice" role="status"><span>{notice}</span></div>
  {/if}

  {#if banner}
    <div class="banner" class:danger={banner.kind === "error" || banner.kind === "deleted"} role="status">
      {#if banner.kind === "recovery"}
        <span>
          {tEditor("recoveryKept", { when: formatDateTime(banner.entry.saved_at) })}
          {#if banner.changedSince}{tEditor("changedSince")}{/if}
        </span>
        <button type="button" onclick={() => void act((s) => s.restoreRecovery())}>{tEditor("restore")}</button>
        <button type="button" onclick={() => void act((s) => s.discardRecovery())}>{tEditor("discard")}</button>
      {:else if banner.kind === "external"}
        {#if banner.confirmReload}
          <span>{tEditor("reloadWarning")}</span>
          <button
            type="button"
            class="strong"
            onclick={() => void act((s) => s.requestReload())}
          >{tEditor("reloadAnyway")}</button>
          <button type="button" onclick={() => void act((s) => s.cancelReload())}>{tEditor("cancel")}</button>
        {:else}
          <span>{tEditor("changedOnDisk")}</span>
          <button type="button" onclick={() => void act((s) => s.requestReload())}>{tEditor("reload")}</button>
          <button type="button" onclick={() => void act((s) => s.keepMine())}>{tEditor("keepMine")}</button>
          <button type="button" onclick={() => void showCompare()}>{tEditor("compare")}</button>
        {/if}
      {:else if banner.kind === "confirmOverwrite"}
        <span>{tEditor("overwriteQuestion")}</span>
        <button type="button" class="strong" onclick={() => void save(true)}>{tEditor("overwrite")}</button>
        <button type="button" onclick={() => void act((s) => s.dismissBanner())}>{tEditor("cancel")}</button>
      {:else if banner.kind === "deleted"}
        <span>{tEditor("deletedOnDisk")}</span>
        <button type="button" onclick={() => void save()}>{tEditor("save")}</button>
        <button type="button" onclick={() => void act((s) => s.dismissBanner())}>{tEditor("dismiss")}</button>
      {:else if banner.kind === "reloaded"}
        <span>{tEditor("reloaded")}</span>
      {:else if banner.kind === "error"}
        <span>{banner.message}</span>
        <button type="button" onclick={() => void act((s) => s.dismissBanner())}>{tEditor("dismiss")}</button>
      {/if}
    </div>
  {/if}

  <div class="body">
    {#if image}
      {#key image}
        <ImageView root={image.root} rel={image.rel} />
      {/key}
    {:else if session}
      <div class="pane" class:gone={previewKind !== null && viewMode === "preview"}>
        {#key session}
          <EditorSurface
            bind:this={surface}
            doc={session.doc}
            {settings}
            fileName={session.fileName}
            revision={outside}
            highlighter={shownHighlighter}
            onTopLine={previewKind === "markdown" && viewMode === "split" ? onSourceTopLine : undefined}
            {onEffect}
            {onChange}
            {onViEffect}
            onViStatus={(s) => (viStatus = s)}
            fileKey={`${session.root}\0${session.rel}`}
            folds={session.folds}
            {onFolds}
            {interceptKey}
            {diagnostics}
            {trail}
            diagnosticsInline={$editorSettings.diagnosticsInline}
            readOnly={session.readOnly}
            hoverAt={lspDoc ? hoverAt : undefined}
            onDefinitionAt={definitionAt}
            onJumpOut={(dir) => walkHistory(dir === -1 ? "back" : "forward")}
            completion={completionPort}
            signature={signaturePort}
            codeActions={codeActionPort}
            breakpoints={breakpointLines}
            {execLine}
            onToggleBreakpoint={debuggable && session ? (line) => toggleDebugBreakpoint(session!.root, session!.rel, line + 1) : undefined}
            conditionalBreakpoints={conditionalLines}
            onBreakpointContext={debuggable ? (line, x, y) => (bpEdit = { line: line + 1, x, y }) : undefined}
          />
        {/key}
      </div>
      {#if bpEdit && session}
        <!-- A breakpoint's condition, hit count and log message (right-click on a line number). -->
        <div class="bp-backdrop" role="presentation" onmousedown={() => (bpEdit = null)}></div>
        <div
          class="bp-popover"
          style:left="{Math.min(bpEdit.x, window.innerWidth - 300)}px"
          style:top="{Math.min(bpEdit.y, window.innerHeight - 280)}px"
        >
          <BreakpointEditor
            info={infoOf($breakpointInfo, session.root, session.rel, bpEdit.line)}
            label="{session.rel.split('/').pop()}:{bpEdit.line}"
            onSave={(info) => session && bpEdit && editDebugBreakpoint(session.root, session.rel, bpEdit.line, info)}
            onClose={() => (bpEdit = null)}
          />
        </div>
      {/if}
      <!-- Positioned in `.body`: the source pane starts at its corner, so the surface's
           pixels apply as they are. -->
      {#if renameBox}
        <input
          bind:this={renameInput}
          bind:value={renameBox.name}
          class="rename"
          style:left="{renameBox.x}px"
          style:top="{renameBox.y}px"
          aria-label={tEditor("newName")}
          spellcheck="false"
          autocomplete="off"
          onkeydown={onRenameKey}
          onblur={() => (renameBox = null)}
        />
      {/if}
      {#if previewKind && viewMode !== "source"}
        <div
          class="pane preview-pane"
          style:--ax-preview-font-size={previewFontSize}
          style:--ax-prose-zoom={previewZoomFactor}
        >
          {#if previewKind === "markdown"}
            <MarkdownPreview
              bind:this={preview}
              text={previewText}
              root={session.root}
              rel={session.rel}
              onTopLine={viewMode === "split" ? onPreviewTopLine : undefined}
              notes={previewNotes}
              onNoteLink={onPreviewLink}
              onTag={showTag}
            />
          {:else if previewKind === "html"}
            <HtmlPreview
              text={previewText}
              rel={session.rel}
              zoom={previewZoomFactor}
              onOpenLink={(rel) => void open({ root: session!.root, rel }, null, "read")}
            />
          {:else}
            <SvgPreview text={previewText} rel={session.rel} />
          {/if}
        </div>
      {/if}
      {#if compare}
        <div class="pane compare">
          <div class="compare-bar">
            <span>{compare.model.hunkCount === 0 ? tEditor("compareSame") : tEditor("compareTitle")}</span>
            <button type="button" onclick={() => (compare = null)}>{tEditor("close")}</button>
          </div>
          <div class="compare-diff">
            {#key compare}
              <DiffPanes
                model={compare.model}
                layout="unified"
                oldText={compare.disk}
                newText={compare.mine}
                fileName={session.fileName}
                onOpen={(line) => goToLine(line)}
              />
            {/key}
          </div>
        </div>
      {/if}
    {:else}
      {@render empty?.()}
    {/if}
  </div>

  {#if session && status}
    <footer>
      {#if viStatus}<ViStatusLine status={viStatus} />{/if}
      <span>{status.position}</span>
      <span>{status.eol}</span>
      <span>{status.indent}</span>
      <span>{wrap ? tEditor("wrapOn") : tEditor("wrapOff")} <kbd>{keyHints("⌥Z", $platform)}</kbd></span>
      {#if previewKind}
        <span>
          {viewModeLabel(viewMode)}
          <kbd>{keyHints("⌘⇧V", $platform)}</kbd>
        </span>
      {/if}
      {#if session.untitled}
        <span class="note">{filing ? tEditor("filingNote") : keyHints(tEditor("newNoteHint"), $platform)}</span>
      {/if}
      {#if session.light}<span class="warn" title={tEditor("largeFileTitle")}>{tEditor("largeFile")}</span>{/if}
      {#if session.readOnly}
        <span class="note" title={session.rel}>{tEditor("readOnlyOutside")}</span>
      {/if}
      {#if diagnostics && diagnostics.size > 0}
        {@const counts = diagnostics.counts()}
        <span class="problems" title={keyHints(tEditor("problemsTitle"), $platform)}>
          {#if counts.error}<span class="diag-error">{"✖"} {counts.error}</span>{/if}
          {#if counts.warning}<span class="diag-warning">{"⚠"} {counts.warning}</span>{/if}
          {#if counts.info + counts.hint}<span class="diag-info">{"ℹ"} {counts.info + counts.hint}</span>{/if}
        </span>
      {/if}
    </footer>
  {/if}
</div>

<style>
  .file-editor {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    height: 100%;
    background: var(--ax-editor-frame-bg);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
  }

  .banner {
    display: flex;
    align-items: center;
    gap: var(--ax-space-3);
    padding: var(--ax-space-2) var(--ax-space-5);
    background: var(--ax-accent-muted);
    border-bottom: 1px solid var(--ax-border);
    font-size: var(--ax-font-size-sm);
  }

  .banner.danger {
    background: var(--ax-surface-3);
    color: var(--ax-danger);
  }

  .banner.notice {
    background: var(--ax-surface-2);
    color: var(--ax-warning);
  }

  .banner span {
    flex: 1;
  }

  .banner button,
  .compare-bar button {
    padding: var(--ax-space-1) var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
    cursor: pointer;
  }

  .banner button.strong {
    border-color: var(--ax-danger);
    color: var(--ax-danger);
  }

  .body {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
  }

  .pane {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .pane.compare,
  .pane.preview-pane {
    border-left: 1px solid var(--ax-border);
  }

  /* Preview only: the source stays mounted (cursor, undo, glide state) but out of sight. */
  .pane.gone {
    display: none;
  }

  .compare-diff {
    position: relative;
    flex: 1;
    min-height: 0;
  }

  .compare-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--ax-space-1) var(--ax-space-3);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    border-bottom: 1px solid var(--ax-border);
  }

  .pane > :global(.surface) {
    flex: 1;
  }

  /* The rename field (L17), at the cursor. */
  .rename {
    position: absolute;
    z-index: 7;
    min-width: 16ch;
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-accent);
    border-radius: var(--ax-radius-sm);
    box-shadow: var(--ax-shadow-pop);
    color: var(--ax-text);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    outline: none;
  }

  .problems {
    display: inline-flex;
    gap: var(--ax-space-2);
  }
  .problems .diag-error {
    color: var(--ax-diag-error);
  }
  .problems .diag-warning {
    color: var(--ax-diag-warning);
  }
  .problems .diag-info {
    color: var(--ax-diag-info);
  }

  footer {
    display: flex;
    gap: var(--ax-space-5);
    padding: var(--ax-space-1) var(--ax-space-5);
    border-top: 1px solid var(--ax-border);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    white-space: nowrap;
    overflow: hidden;
  }

  kbd {
    margin-left: var(--ax-space-1);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
  }

  .warn {
    color: var(--ax-warning);
  }

  .note {
    color: var(--ax-accent);
  }

  /* Vi's mode pill (V8): one colour per mode, Normal in the accent. */

  .bp-backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
  }

  .bp-popover {
    position: fixed;
    z-index: 41;
    width: calc(280px * var(--ax-ui-scale));
    padding: var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    box-shadow: var(--ax-shadow-lg, 0 8px 24px rgba(0, 0, 0, 0.4));
  }
</style>
