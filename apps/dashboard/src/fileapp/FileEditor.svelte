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
-->
<script lang="ts" module>
  /** What the owner shows about the open file (a title, the unsaved dot). */
  export interface OpenFileState {
    root: string;
    rel: string;
    dirty: boolean;
  }

  /** How `open` went; a failure keeps the file that was open before. */
  export type OpenResult = { ok: true } | { ok: false; kind: string | null; message: string };
</script>

<script lang="ts">
  import { onMount, tick as nextTick, type Snippet } from "svelte";

  import { listenBackend, type FileChange } from "../core/backend";
  import { messageOf } from "../core/errors";
  import { hunksFromTexts } from "../editor/diff/hunks";
  import { DiffModel, textLines } from "../editor/diff/model";
  import type { Indent } from "../editor/detect";
  import type { Effect } from "../editor/keymap";
  import { SyntaxHighlighter } from "../editor/syntax/highlighter";
  import { detectLanguage } from "../editor/syntax/languages";
  import { fileBackend } from "./backend";
  import DiffPanes from "./DiffPanes.svelte";
  import { editorFace } from "./editorFace.svelte";
  import { editorSettings, ensureEditorSettingsLoaded } from "./editorSettings";
  import EditorSettingsPanel from "./EditorSettingsPanel.svelte";
  import EditorSurface from "./EditorSurface.svelte";
  import { grammarRuntime } from "./grammars";
  import MarkdownPreview from "./MarkdownPreview.svelte";
  import { ScrollLink } from "./scrollLink";
  import { FileSession } from "./session";
  import { statusParts } from "./status";
  import { surfaceSettings, wrapsByDefault } from "./surfaceSettings";

  interface Props {
    /** On screen: leaving it is a moment for "autosave when leaving". */
    visible?: boolean;
    /** The settings panel beside the text (the file app's gear). */
    showSettings?: boolean;
    onCloseSettings?: () => void;
    /** A hint above the text, from the owner (G6: the agent is working in this worktree). */
    notice?: string | null;
    /** ⌘O — the owner decides what opening another file means. */
    onOpenRequest?: () => void;
    /** After every change of the open file or its state. */
    onState?: (state: OpenFileState | null) => void;
    /** Shown while no file is open. */
    empty?: Snippet;
  }

  let {
    visible = true,
    showSettings = false,
    onCloseSettings,
    notice = null,
    onOpenRequest,
    onState,
    empty,
  }: Props = $props();

  /** Quiet time after the last change before unsaved text is kept aside (F8). */
  const RECOVERY_DELAY_MS = 2000;
  /** How long the "reloaded" hint stays. */
  const HINT_MS = 3000;
  /** Quiet time before "autosave after a pause" saves (F9). */
  const AUTOSAVE_DELAY_MS = 1000;

  let session = $state.raw<FileSession | null>(null);
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
  /** Syntax colours for the open file (ED2); `null` for plain text or a large file. */
  let highlighter = $state.raw<SyntaxHighlighter | null>(null);
  /** Markdown files only (G8): source, the rendered preview, or both side by side. */
  let mdMode = $state<"source" | "preview" | "split">("source");
  let isMarkdown = $state(false);
  let preview = $state<MarkdownPreview | null>(null);
  /** Source and preview scroll in step; whichever is scrolled leads (H12). */
  const scrollLink = new ScrollLink();
  let recoveryTimer: ReturnType<typeof setTimeout> | null = null;
  let autosaveTimer: ReturnType<typeof setTimeout> | null = null;
  let hintTimer: ReturnType<typeof setTimeout> | null = null;
  /** The font face the surfaces draw with — switched only once it has loaded. */
  const face = editorFace();

  const settings = $derived({
    ...surfaceSettings($editorSettings, wrap),
    fontFamily: face.family,
    fontWeight: face.weight,
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

  $effect(() => {
    void sessionTick;
    const s = session;
    onState?.(s ? { root: s.root, rel: s.rel, dirty: s.doc.dirty } : null);
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
   * only once the new one has opened — and puts the cursor on `line`.
   */
  export async function open(file: { root: string; rel: string }, line: number | null = null): Promise<OpenResult> {
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
    await leaveCurrent();
    session = next;
    wrap = wrapsByDefault($editorSettings, next.fileName);
    isMarkdown = detectLanguage(next.fileName, next.doc.store.line(0)) === "markdown";
    mdMode = "source";
    compare = null;
    refresh();
    void attachHighlighter(next);
    await nextTick();
    if (line !== null) goToLine(line);
    else surface?.focus();
    return { ok: true };
  }

  /** Puts the cursor on `line` (zero-based) and focuses the text. */
  export function goToLine(line: number): void {
    surface?.goToLine(line);
    surface?.focus();
  }

  export function focus(): void {
    surface?.focus();
  }

  /**
   * Starts highlighting `s` once its grammar has loaded (G4): nothing for plain
   * text, and nothing for a large read-only file. An injected language that
   * arrives later redraws through `outside`.
   */
  async function attachHighlighter(s: FileSession): Promise<void> {
    if (s.readOnly) return;
    const id = detectLanguage(s.fileName, s.doc.store.line(0));
    if (!id) return;
    const created = await SyntaxHighlighter.create(s.doc, grammarRuntime, id, () => outside++);
    if (session !== s) {
      created?.dispose();
      return;
    }
    highlighter = created;
  }

  /** Keeps unsaved text aside and stops watching the file being left. */
  async function leaveCurrent(): Promise<void> {
    highlighter?.dispose();
    highlighter = null;
    if (recoveryTimer) clearTimeout(recoveryTimer);
    recoveryTimer = null;
    if (!session) return;
    if ($editorSettings.autosave !== "off") await session.save();
    await session.persistRecovery();
    await session.close();
  }

  async function save(confirmed = false): Promise<void> {
    if (!session) return;
    await session.save(confirmed);
    refresh();
  }

  /** ⌘⇧V: source → preview → side by side → source (G8). */
  function cycleMarkdownMode(): void {
    if (!isMarkdown) return;
    mdMode = mdMode === "source" ? "preview" : mdMode === "preview" ? "split" : "source";
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
    return isMarkdown && mdMode !== "source" && session ? session.doc.store.text() : "";
  });

  function onEffect(effect: Effect): void {
    if (effect === "togglePreview") cycleMarkdownMode();
    else if (effect === "save") void save();
    else if (effect === "open") onOpenRequest?.();
    else if (effect === "toggleWrap") wrap = !wrap;
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

  /**
   * ⌘S and ⌘⇧V for the whole editor, so they also work while focus is on a
   * banner button. Handled in the capture phase and stopped there: the
   * surface would otherwise see the same key and save a second time.
   */
  function onKeydownCapture(e: KeyboardEvent): void {
    if (!e.metaKey || e.altKey || e.ctrlKey) return;
    const key = e.key.toLowerCase();
    const previewKey = key === "v" && e.shiftKey && isMarkdown;
    if (key !== "s" && !previewKey) return;
    e.preventDefault();
    e.stopPropagation();
    if (previewKey) cycleMarkdownMode();
    else void save();
  }

  $effect(() => {
    if (!visible) saveOnLeave();
  });

  onMount(() => {
    void ensureEditorSettingsLoaded();
    const unlisten = listenBackend<FileChange>("files:changed", (change) => {
      void session?.onExternalChange(change).then((shown) => shown && refresh());
    });
    window.addEventListener("blur", saveOnLeave);
    return () => {
      window.removeEventListener("blur", saveOnLeave);
      void unlisten.then((off) => off());
      if (hintTimer) clearTimeout(hintTimer);
      if (autosaveTimer) clearTimeout(autosaveTimer);
      // Going away is leaving the file: keep unsaved text aside, stop watching.
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
          Unsaved changes from {new Date(banner.entry.saved_at).toLocaleString()} were kept.
          {#if banner.changedSince}The file has changed on disk since.{/if}
        </span>
        <button type="button" onclick={() => void act((s) => s.restoreRecovery())}>Restore</button>
        <button type="button" onclick={() => void act((s) => s.discardRecovery())}>Discard</button>
      {:else if banner.kind === "external"}
        {#if banner.confirmReload}
          <span>Reloading throws your unsaved changes away.</span>
          <button type="button" class="strong" onclick={() => void act((s) => s.requestReload())}>Reload anyway</button>
          <button type="button" onclick={() => void act((s) => s.cancelReload())}>Cancel</button>
        {:else}
          <span>The file changed on disk while you have unsaved changes.</span>
          <button type="button" onclick={() => void act((s) => s.requestReload())}>Reload</button>
          <button type="button" onclick={() => void act((s) => s.keepMine())}>Keep mine</button>
          <button type="button" onclick={() => void showCompare()}>Compare</button>
        {/if}
      {:else if banner.kind === "confirmOverwrite"}
        <span>The file was changed on disk. Save anyway and overwrite it?</span>
        <button type="button" class="strong" onclick={() => void save(true)}>Overwrite</button>
        <button type="button" onclick={() => void act((s) => s.dismissBanner())}>Cancel</button>
      {:else if banner.kind === "deleted"}
        <span>The file was deleted on disk. Saving creates it again.</span>
        <button type="button" onclick={() => void save()}>Save</button>
        <button type="button" onclick={() => void act((s) => s.dismissBanner())}>Dismiss</button>
      {:else if banner.kind === "reloaded"}
        <span>Reloaded — the file changed on disk.</span>
      {:else if banner.kind === "error"}
        <span>{banner.message}</span>
        <button type="button" onclick={() => void act((s) => s.dismissBanner())}>Dismiss</button>
      {/if}
    </div>
  {/if}

  <div class="body">
    {#if showSettings}
      <EditorSettingsPanel surface={settings} onClose={() => onCloseSettings?.()} />
    {/if}
    {#if session}
      <div class="pane" class:gone={isMarkdown && mdMode === "preview"}>
        {#key session}
          <EditorSurface
            bind:this={surface}
            doc={session.doc}
            {settings}
            fileName={session.fileName}
            readOnly={session.readOnly}
            revision={outside}
            {highlighter}
            onTopLine={isMarkdown && mdMode === "split" ? onSourceTopLine : undefined}
            {onEffect}
            {onChange}
          />
        {/key}
      </div>
      {#if isMarkdown && mdMode !== "source"}
        <div class="pane preview-pane">
          <MarkdownPreview
            bind:this={preview}
            text={previewText}
            root={session.root}
            rel={session.rel}
            onTopLine={mdMode === "split" ? onPreviewTopLine : undefined}
          />
        </div>
      {/if}
      {#if compare}
        <div class="pane compare">
          <div class="compare-bar">
            <span>On disk → your text {compare.model.hunkCount === 0 ? "(no difference)" : ""}</span>
            <button type="button" onclick={() => (compare = null)}>Close</button>
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
      <span>{status.position}</span>
      <span>{status.eol}</span>
      <span>{status.indent}</span>
      <span>{wrap ? "Wrap on" : "Wrap off"} <kbd>⌥Z</kbd></span>
      {#if isMarkdown}
        <span>
          {mdMode === "source" ? "Source" : mdMode === "preview" ? "Preview" : "Side by side"}
          <kbd>⌘⇧V</kbd>
        </span>
      {/if}
      {#if session.readOnly}<span class="warn">Read-only (large file)</span>{/if}
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
    background: var(--ax-bg);
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
</style>
