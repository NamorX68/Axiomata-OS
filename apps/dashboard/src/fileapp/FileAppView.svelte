<!--
  The file app's full-screen view (`docs/plans/editor.md`, ED1.3, F7, ED4.3):
  the title, opening files with the native dialog (⌘O) or from the recent
  list, the settings gear — and the tabs (W7, W12), each a `FileTab` with its
  own `FileEditor`, the same editor the panel and the IDE's file pane use.

  * **Hidden, never unmounted** — the same rule as the IDE: `App.svelte` keeps
    it mounted once opened, so unsaved text and the cursor survive a trip back
    to the OS. `inert` keeps the hidden view out of focus and tab order. The
    tabs behind the front one stay mounted the same way.
  * **Tabs** (`tabs.ts`): one per file, a reusable preview tab, kept across
    restarts; ⌘W closes (asking first over unsaved text), ⌘N a new note,
    ⌃Tab/⌃⇧Tab and ⌘1–⌘9 switch. Editing in the preview tab fixes it.
  * **A panel hands its file over** (`handoff.ts`, W11): the tab takes the
    panel's live session, unsaved text and undo included.
  * **A file that cannot be opened is forgotten** from the recent list when it
    is gone or its root is; any other failure only says so. Its tab closes.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import type { FileRootInfo } from "../core/backend";
  import { listRoots, pickFile } from "./backend";
  import type { OpenFileState, OpenResult } from "./FileEditor.svelte";
  import FileTab from "./FileTab.svelte";
  import { handoffs, takeHandoffs, type Handoff } from "./handoff";
  import { forgetRecent, recentFiles, rememberRecent, type RecentFile } from "./recent";
  import {
    closeTab,
    cycleTab,
    loadTabs,
    NO_TABS,
    nthTab,
    openInTabs,
    pinTab,
    retargetTab,
    saveTabs,
    type FileRef,
    type Tab,
    type TabsState,
  } from "./tabs";
  import UnsavedQuestion from "./UnsavedQuestion.svelte";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  let tabs = $state<TabsState>(NO_TABS);
  /** What each tab's editor reports — for titles, the unsaved dots, the header. */
  let states = $state<Record<string, OpenFileState | null>>({});
  let views = $state<Record<string, FileTab | null>>({});
  /** A panel's session waiting for the tab it was handed to (read once, when the tab loads). */
  const handedTo = new Map<string, Handoff["handed"]>();
  /** The tab being closed while it asks about unsaved text. */
  let closing = $state<{ id: string; answer: (close: boolean) => void } | null>(null);
  /** Tabs are saved only once the saved ones were read back, never over them (tracked: the save waits for it). */
  let restored = $state(false);

  let error = $state("");
  let recent = $state<RecentFile[]>([]);
  let roots = $state<FileRootInfo[]>([]);
  let showRecent = $state(false);
  let showSettings = $state(false);

  const newId = () => crypto.randomUUID();
  const active = $derived(tabs.tabs.find((t) => t.id === tabs.active) ?? null);
  const current = $derived(active ? (states[active.id] ?? null) : null);

  function rootLabel(id: string): string {
    return roots.find((r) => r.id === id)?.label ?? id;
  }

  function tabTitle(tab: Tab): string {
    if (tab.file === null || states[tab.id]?.untitled) return "New note";
    return tab.file.rel.split("/").pop() ?? tab.file.rel;
  }

  /** Opens `file` (`null`: a new note) in a tab — the preview tab when only looking (W7). */
  function openTab(file: FileRef | null, preview = false, handed: Handoff["handed"] = null): void {
    showRecent = false;
    error = "";
    const result = openInTabs(tabs, file, { preview }, newId);
    if (handed && result.load) handedTo.set(result.target, handed);
    else if (handed) void keepAside(handed);
    tabs = result.state;
    if (file) {
      rememberRecent(file);
      recent = recentFiles();
    }
  }

  /** A handed-over session whose file already has a tab: its unsaved text is kept aside, not lost. */
  async function keepAside(handed: NonNullable<Handoff["handed"]>): Promise<void> {
    await handed.session.persistRecovery();
    await handed.session.close();
  }

  function openFile(file: RecentFile): void {
    openTab({ root: file.root, rel: file.rel });
  }

  async function openPicked(): Promise<void> {
    showRecent = false;
    let picked;
    try {
      picked = await pickFile();
    } catch (err) {
      error = `Could not open the dialog: ${(err as { message?: string }).message ?? String(err)}`;
      return;
    }
    if (picked && !picked.folder) openTab({ root: picked.root, rel: picked.rel });
  }

  function onTabState(tab: Tab, state: OpenFileState | null): void {
    states[tab.id] = state;
    // Editing in the preview tab makes it a tab of its own (W7).
    if (state?.dirty && tab.preview) tabs = pinTab(tabs, tab.id);
  }

  function onTabFailed(tab: Tab, result: Extract<OpenResult, { ok: false }>): void {
    if (tab.file) {
      error = `Could not open ${tab.file.rel}: ${result.message}`;
      if (result.kind === "NotFound" || result.kind === "UnknownRoot") forgetRecent(tab.file);
      recent = recentFiles();
    }
    dropTab(tab.id);
  }

  function dropTab(id: string): void {
    // A hand-over no editor took (the tab closed before it mounted) must not keep its file
    // watched; one that was taken belongs to that editor, which keeps its text aside itself.
    if (!views[id]) void handedTo.get(id)?.session.close();
    handedTo.delete(id);
    tabs = closeTab(tabs, id);
    delete states[id];
    delete views[id];
  }

  /** ⌘W, ×, Vi's `:q`: closes tab `id`, asking first over unsaved text (W11). */
  async function requestCloseTab(id: string): Promise<void> {
    // One question at a time: a second × while one is asked would leave the first unanswered.
    if (closing) return;
    const view = views[id];
    if (view?.hasUnsaved()) {
      tabs = { ...tabs, active: id };
      const close = await new Promise<boolean>((resolve) => {
        closing = {
          id,
          answer: (answer) => {
            closing = null;
            resolve(answer);
          },
        };
      });
      if (!close) return;
    }
    dropTab(id);
  }

  async function saveAndClose(): Promise<void> {
    const c = closing;
    if (c && (await views[c.id]?.saveNow())) c.answer(true);
  }

  async function discardAndClose(): Promise<void> {
    const c = closing;
    if (!c) return;
    await views[c.id]?.discard();
    c.answer(true);
  }

  /**
   * The view's own keys (W12), in the capture phase so they work with the
   * editor focused or not — and stopped there, like the editor's own ⌘S.
   */
  function onViewKeydown(e: KeyboardEvent): void {
    const key = e.key.toLowerCase();
    if (e.ctrlKey && !e.metaKey && !e.altKey && e.key === "Tab") {
      take(e);
      tabs = cycleTab(tabs, e.shiftKey ? -1 : 1);
      return;
    }
    if (!e.metaKey || e.altKey || e.ctrlKey) return;
    if (!e.shiftKey && /^[1-9]$/.test(key)) {
      take(e);
      tabs = nthTab(tabs, Number(key));
    } else if (e.shiftKey) {
      return;
    } else if (key === "o") {
      take(e);
      void openPicked();
    } else if (key === "n") {
      take(e);
      openTab(null);
    } else if (key === "w") {
      take(e);
      if (tabs.active) void requestCloseTab(tabs.active);
    }
  }

  function take(e: KeyboardEvent): void {
    e.preventDefault();
    e.stopPropagation();
  }

  /** Files handed over from a panel (W11), in the order they came. */
  function takeWaiting(): void {
    for (const h of takeHandoffs()) openTab(h.file, false, h.handed);
  }

  onMount(() => {
    tabs = loadTabs(newId);
    restored = true;
    takeWaiting();
    return handoffs.subscribe((list) => {
      if (list.length > 0) takeWaiting();
    });
  });

  $effect(() => {
    const snapshot = tabs;
    if (!restored) return;
    saveTabs(snapshot);
  });

  $effect(() => {
    if (open) {
      recent = recentFiles();
      void listRoots()
        .then((list) => (roots = list))
        .catch(() => undefined);
    }
  });
</script>

<section class="files" class:hidden={!open} inert={!open} aria-label="Editor" onkeydowncapture={onViewKeydown}>
  <header>
    <div class="titles">
      <h1>Editor</h1>
      {#if current && !current.untitled}
        <span class="path" title={current.rel}>
          <span class="root">{rootLabel(current.root)}</span> / {current.rel}
          {#if current.dirty}<span class="dirty" aria-label="Unsaved changes">●</span>{/if}
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
                <button type="button" role="menuitem" onclick={() => openFile(file)}>
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

  {#if tabs.tabs.length > 0}
    <div class="tabbar" role="tablist" aria-label="Open files">
      {#each tabs.tabs as tab (tab.id)}
        <div class="tab" class:active={tab.id === tabs.active} class:preview={tab.preview}>
          <button
            type="button"
            role="tab"
            class="tab-title"
            aria-selected={tab.id === tabs.active}
            title={tab.file ? `${rootLabel(tab.file.root)} / ${tab.file.rel}` : "New note"}
            onclick={() => (tabs = { ...tabs, active: tab.id })}
            ondblclick={() => (tabs = pinTab(tabs, tab.id))}
          >
            {tabTitle(tab)}
            {#if states[tab.id]?.dirty}<span class="dirty" aria-label="Unsaved changes">●</span>{/if}
          </button>
          <button
            type="button"
            class="tab-close"
            aria-label="Close {tabTitle(tab)}"
            onclick={() => void requestCloseTab(tab.id)}>×</button
          >
        </div>
      {/each}
    </div>
  {/if}

  {#if error}
    <div class="banner danger" role="alert">
      <span>{error}</span>
      <button type="button" onclick={() => (error = "")}>Dismiss</button>
    </div>
  {/if}

  {#if closing}
    <UnsavedQuestion
      name={states[closing.id]?.rel ?? "This file"}
      untitled={states[closing.id]?.untitled ?? false}
      onSave={() => void saveAndClose()}
      onDiscard={() => void discardAndClose()}
      onCancel={() => closing?.answer(false)}
    />
  {/if}

  <div class="stack">
    {#each tabs.tabs as tab (tab.id)}
      <FileTab
        bind:this={views[tab.id]}
        file={tab.file}
        handed={handedTo.get(tab.id) ?? null}
        visible={open && tab.id === tabs.active}
        {showSettings}
        onCloseSettings={() => (showSettings = false)}
        onOpenRequest={() => void openPicked()}
        onQuit={() => void requestCloseTab(tab.id)}
        onState={(state) => onTabState(tab, state)}
        onMoved={(file) => (tabs = retargetTab(tabs, tab.id, file))}
        onFailed={(result) => onTabFailed(tab, result)}
      />
    {:else}
      <div class="empty">
        <p>No file open.</p>
        <button type="button" class="pill" onclick={() => void openPicked()}>Open a file… <kbd>⌘O</kbd></button>
        <button type="button" class="pill" onclick={() => openTab(null)}>New note <kbd>⌘N</kbd></button>
        {#if recent.length > 0}
          <ul class="recent-inline">
            {#each recent as file (file.root + file.rel)}
              <li>
                <button type="button" onclick={() => openFile(file)}>
                  {file.rel} <small>{rootLabel(file.root)}</small>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {/each}
  </div>
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
  .banner button {
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

  .banner button {
    font-size: var(--ax-font-size-xs);
  }

  .gear {
    font-size: var(--ax-font-size-base);
    line-height: 1;
  }

  .gear[aria-pressed="true"] {
    border-color: var(--ax-accent);
  }

  /* The tabs lie on top of each other here; only the front one is visible (FileTab). */
  .stack {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
  }

  .tabbar {
    display: flex;
    gap: 1px;
    overflow-x: auto;
    background: var(--ax-border);
    border-bottom: 1px solid var(--ax-border);
  }

  .tab {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    max-width: 240px;
    background: var(--ax-surface-1);
  }

  .tab.active {
    background: var(--ax-bg);
    box-shadow: inset 0 -2px 0 var(--ax-accent);
  }

  .tab-title,
  .tab-close {
    background: none;
    border: 0;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }

  .tab-title {
    overflow: hidden;
    padding: var(--ax-space-2) var(--ax-space-2) var(--ax-space-2) var(--ax-space-4);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tab.active .tab-title {
    color: var(--ax-text);
  }

  /* The preview tab (W7): italic until it becomes a tab of its own. */
  .tab.preview .tab-title {
    font-style: italic;
  }

  .tab-close {
    padding: var(--ax-space-2) var(--ax-space-3) var(--ax-space-2) var(--ax-space-1);
  }

  .tab-close:hover {
    color: var(--ax-text);
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
</style>
