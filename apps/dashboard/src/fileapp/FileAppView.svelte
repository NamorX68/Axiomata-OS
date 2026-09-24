<!--
  The file app's full-screen view (`docs/plans/editor.md`, ED1.3, F7–F10): one
  file at a time in the editor surface, opened with the native dialog (⌘O) or
  from the recent list, saved with ⌘S.

  * **Hidden, never unmounted** — the same rule as the IDE: `App.svelte` keeps
    it mounted once opened, so unsaved text and the cursor survive a trip back
    to the OS. `inert` keeps the hidden view out of focus and tab order.
  * **All file logic is in `FileSession`** (tested without a DOM); this view
    only shows its banner and state and forwards the owner's choices.
  * **Unsaved text is kept aside two seconds after the last change** (F8), and
    whenever a file is left for another one.
  * **Settings come from `editor-settings.json`** (gear → `EditorSettingsPanel`).
    A font face is loaded before the surface gets it, so the surface never
    measures its cell width on a fallback font.
  * **Autosave (F9)** saves 1 s after the last change or when the view is left or
    the window loses focus — always with the expected version, so it stops at a
    conflict instead of overwriting.
-->
<script lang="ts">
  import { onMount, tick as nextTick } from "svelte";

  import { listenBackend, type FileChange, type FileRootInfo } from "../core/backend";
  import { hunksFromTexts } from "../editor/diff/hunks";
  import { DiffModel, textLines } from "../editor/diff/model";
  import type { Indent } from "../editor/detect";
  import type { Effect } from "../editor/keymap";
  import { SyntaxHighlighter } from "../editor/syntax/highlighter";
  import { detectLanguage } from "../editor/syntax/languages";
  import { fileBackend, listRoots, pickFile } from "./backend";
  import DiffPanes from "./DiffPanes.svelte";
  import { editorSettings, ensureEditorSettingsLoaded } from "./editorSettings";
  import EditorSettingsPanel from "./EditorSettingsPanel.svelte";
  import EditorSurface from "./EditorSurface.svelte";
  import MarkdownPreview from "./MarkdownPreview.svelte";
  import { editorFace } from "./editorFace.svelte";
  import { grammarRuntime } from "./grammars";
  import { forgetRecent, recentFiles, rememberRecent, type RecentFile } from "./recent";
  import { ScrollLink } from "./scrollLink";
  import { FileSession } from "./session";
  import { statusParts } from "./status";
  import { surfaceSettings, wrapsByDefault } from "./surfaceSettings";

  let { open = $bindable(false) }: { open?: boolean } = $props();

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
  let error = $state("");
  let wrap = $state(false);
  /** "Show difference" (F10, H7): the file on disk against the text being edited, as a diff. */
  let compare = $state.raw<{ model: DiffModel; disk: string; mine: string } | null>(null);
  let recent = $state<RecentFile[]>([]);
  let roots = $state<FileRootInfo[]>([]);
  let showRecent = $state(false);
  let surface = $state<EditorSurface | null>(null);
  let showSettings = $state(false);
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
  const dirty = $derived.by(() => {
    void sessionTick;
    return session?.doc.dirty ?? false;
  });
  const status = $derived.by(() => {
    void sessionTick;
    return session ? statusParts(session.doc, settings.tabSize) : null;
  });

  function rootLabel(id: string): string {
    return roots.find((r) => r.id === id)?.label ?? id;
  }

  /** A human-readable message from a backend error, whatever shape it arrives in. */
  function errorMessage(err: unknown): string {
    return (err as { message?: string }).message ?? String(err);
  }

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

  async function openFile(file: RecentFile): Promise<void> {
    showRecent = false;
    error = "";
    let next: FileSession;
    try {
      next = await FileSession.open(fileBackend, file.root, file.rel, indentFallback());
    } catch (err) {
      const kind = (err as { kind?: string }).kind;
      error = `Could not open ${file.rel}: ${errorMessage(err)}`;
      if (kind === "NotFound" || kind === "UnknownRoot") {
        forgetRecent(file);
        recent = recentFiles();
      }
      return;
    }
    await leaveCurrent();
    session = next;
    wrap = wrapsByDefault($editorSettings, next.fileName);
    isMarkdown = detectLanguage(next.fileName, next.doc.store.line(0)) === "markdown";
    mdMode = "source";
    compare = null;
    rememberRecent(file);
    recent = recentFiles();
    refresh();
    void attachHighlighter(next);
    await nextTick();
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

  async function openPicked(): Promise<void> {
    showRecent = false;
    let picked;
    try {
      picked = await pickFile();
    } catch (err) {
      error = `Could not open the dialog: ${errorMessage(err)}`;
      return;
    }
    if (picked && !picked.folder) await openFile({ root: picked.root, rel: picked.rel });
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
    else if (effect === "open") void openPicked();
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

  /** "Autosave when leaving": the view closes or the window loses focus. */
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
   * ⌘O and ⌘S for the whole view, so they also work with no file open or the
   * editor unfocused. Handled in the capture phase and stopped there: the
   * surface would otherwise see the same key and save a second time.
   */
  function onViewKeydown(e: KeyboardEvent): void {
    if (!e.metaKey || e.altKey || e.ctrlKey) return;
    const key = e.key.toLowerCase();
    const preview = key === "v" && e.shiftKey && isMarkdown;
    if (key !== "o" && key !== "s" && !preview) return;
    e.preventDefault();
    e.stopPropagation();
    if (preview) cycleMarkdownMode();
    else if (key === "o") void openPicked();
    else void save();
  }

  $effect(() => {
    if (!open) saveOnLeave();
  });

  $effect(() => {
    if (open) {
      recent = recentFiles();
      void listRoots()
        .then((list) => (roots = list))
        .catch(() => undefined);
    }
  });

  onMount(() => {
    void ensureEditorSettingsLoaded();
    const unlisten = listenBackend<FileChange>("files:changed", (change) => {
      void session?.onExternalChange(change).then((visible) => visible && refresh());
    });
    window.addEventListener("blur", saveOnLeave);
    return () => {
      window.removeEventListener("blur", saveOnLeave);
      void unlisten.then((off) => off());
    };
  });
</script>

<section class="files" class:hidden={!open} inert={!open} aria-label="Editor" onkeydowncapture={onViewKeydown}>
  <header>
    <div class="titles">
      <h1>Editor</h1>
      {#if session}
        <span class="path" title={session.rel}>
          <span class="root">{rootLabel(session.root)}</span> / {session.rel}
          {#if dirty}<span class="dirty" aria-label="Unsaved changes">●</span>{/if}
        </span>
      {/if}
    </div>
    <div class="actions">
      <button type="button" class="pill" onclick={() => void openPicked()}>Open… <kbd>⌘O</kbd></button>
      <div class="recent-anchor">
        <button type="button" class="pill" disabled={recent.length === 0} onclick={() => (showRecent = !showRecent)}>
          Recent ▾
        </button>
        {#if showRecent}
          <ul class="recent" role="menu">
            {#each recent as file (file.root + file.rel)}
              <li>
                <button type="button" role="menuitem" onclick={() => void openFile(file)}>
                  <span>{file.rel.split("/").pop()}</span>
                  <small>{rootLabel(file.root)} / {file.rel}</small>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
      <button
        type="button"
        class="pill gear"
        aria-label="Editor settings"
        aria-pressed={showSettings}
        onclick={() => (showSettings = !showSettings)}>⚙</button
      >
      <button type="button" class="pill back" onclick={() => (open = false)}>Back to the OS</button>
    </div>
  </header>

  {#if error}
    <div class="banner danger" role="alert">
      <span>{error}</span>
      <button type="button" onclick={() => (error = "")}>Dismiss</button>
    </div>
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
      <EditorSettingsPanel surface={settings} onClose={() => (showSettings = false)} />
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
                onOpen={(line) => surface?.goToLine(line)}
              />
            {/key}
          </div>
        </div>
      {/if}
    {:else}
      <div class="empty">
        <p>No file open.</p>
        <button type="button" class="pill" onclick={() => void openPicked()}>Open a file… <kbd>⌘O</kbd></button>
        {#if recent.length > 0}
          <ul class="recent-inline">
            {#each recent as file (file.root + file.rel)}
              <li>
                <button type="button" onclick={() => void openFile(file)}>
                  {file.rel} <small>{rootLabel(file.root)}</small>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
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
</section>

<style>
  .files {
    position: fixed;
    inset: 0;
    z-index: calc(var(--ax-z-staging) - 1);
    display: flex;
    flex-direction: column;
    background: var(--ax-bg);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
  }

  /* Hidden, not unmounted — see the header comment. */
  .files.hidden {
    visibility: hidden;
    pointer-events: none;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ax-space-4);
    padding: var(--ax-space-3) var(--ax-space-5);
    border-bottom: 1px solid var(--ax-border);
  }

  .titles {
    display: flex;
    align-items: baseline;
    gap: var(--ax-space-3);
    min-width: 0;
  }

  h1 {
    margin: 0;
    font-family: var(--ax-font-display);
    font-size: var(--ax-font-size-lg);
    letter-spacing: var(--ax-tracking-wide);
  }

  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }

  .root {
    color: var(--ax-text);
  }

  .dirty {
    margin-left: var(--ax-space-2);
    color: var(--ax-accent);
  }

  .actions {
    display: flex;
    gap: var(--ax-space-2);
    align-items: center;
  }

  /* Shared shape for every pill-style button in this view; each caller only adds its own font size. */
  .pill,
  .banner button,
  .compare-bar button {
    padding: var(--ax-space-1) var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    cursor: pointer;
  }

  .pill {
    font-size: var(--ax-font-size-sm);
  }

  .pill:hover:not(:disabled) {
    border-color: var(--ax-accent);
  }

  .pill:disabled {
    opacity: var(--ax-tile-glass-opacity);
    cursor: default;
  }

  .back {
    color: var(--ax-text-muted);
  }

  kbd {
    margin-left: var(--ax-space-1);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
  }

  .recent-anchor {
    position: relative;
  }

  .recent {
    position: absolute;
    right: 0;
    top: calc(100% + var(--ax-space-1));
    z-index: 10;
    min-width: 320px;
    margin: 0;
    padding: var(--ax-space-1);
    list-style: none;
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    box-shadow: var(--ax-shadow-pop);
  }

  .recent button,
  .recent-inline button {
    display: flex;
    flex-direction: column;
    width: 100%;
    padding: var(--ax-space-1) var(--ax-space-2);
    background: none;
    border: 0;
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    text-align: left;
    cursor: pointer;
  }

  .recent button:hover,
  .recent-inline button:hover {
    background: var(--ax-accent-muted);
  }

  small {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
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

  .banner span {
    flex: 1;
  }

  .banner button,
  .compare-bar button {
    font-size: var(--ax-font-size-xs);
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

  .gear {
    font-size: var(--ax-font-size-base);
    line-height: 1;
  }

  .gear[aria-pressed="true"] {
    border-color: var(--ax-accent);
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

  .empty {
    margin: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--ax-space-3);
    color: var(--ax-text-muted);
  }

  .recent-inline {
    margin: 0;
    padding: 0;
    list-style: none;
    min-width: 360px;
  }

  footer {
    display: flex;
    gap: var(--ax-space-5);
    padding: var(--ax-space-1) var(--ax-space-5);
    border-top: 1px solid var(--ax-border);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .warn {
    color: var(--ax-warning);
  }
</style>
