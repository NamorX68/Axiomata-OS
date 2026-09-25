<!--
  The file app's full-screen view (`docs/plans/editor.md`, ED1.3, F7): the
  title, opening files with the native dialog (⌘O) or from the recent list, the
  settings gear — and `FileEditor`, which does all the editing (F8–F10, ED2),
  the same component the IDE's file pane uses.

  * **Hidden, never unmounted** — the same rule as the IDE: `App.svelte` keeps
    it mounted once opened, so unsaved text and the cursor survive a trip back
    to the OS. `inert` keeps the hidden view out of focus and tab order.
  * **A file that cannot be opened is forgotten** from the recent list when it
    is gone or its root is; any other failure only says so.
-->
<script lang="ts">
  import type { FileRootInfo } from "../core/backend";
  import { listRoots, pickFile } from "./backend";
  import FileEditor, { type OpenFileState } from "./FileEditor.svelte";
  import { forgetRecent, recentFiles, rememberRecent, type RecentFile } from "./recent";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  let editor = $state<FileEditor | null>(null);
  /** The open file, as the editor reports it — for the title and the unsaved dot. */
  let current = $state<OpenFileState | null>(null);
  let error = $state("");
  let recent = $state<RecentFile[]>([]);
  let roots = $state<FileRootInfo[]>([]);
  let showRecent = $state(false);
  let showSettings = $state(false);

  function rootLabel(id: string): string {
    return roots.find((r) => r.id === id)?.label ?? id;
  }

  /** Opens `file` in the editor; a file that is gone is also forgotten from the recent list. */
  async function openFile(file: RecentFile): Promise<void> {
    showRecent = false;
    error = "";
    const result = (await editor?.open(file)) ?? { ok: false, kind: null, message: "the editor is not ready" };
    if (!result.ok) {
      error = `Could not open ${file.rel}: ${result.message}`;
      if (result.kind === "NotFound" || result.kind === "UnknownRoot") forgetRecent(file);
    } else {
      rememberRecent(file);
    }
    recent = recentFiles();
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
    if (picked && !picked.folder) await openFile({ root: picked.root, rel: picked.rel });
  }

  /**
   * ⌘O for the whole view, so it also works with no file open or the editor
   * unfocused. Handled in the capture phase and stopped there, like the
   * editor's own ⌘S.
   */
  function onViewKeydown(e: KeyboardEvent): void {
    if (!e.metaKey || e.altKey || e.ctrlKey || e.shiftKey || e.key.toLowerCase() !== "o") return;
    e.preventDefault();
    e.stopPropagation();
    void openPicked();
  }

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
      {#if current}
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

  <FileEditor
    bind:this={editor}
    visible={open}
    {showSettings}
    onCloseSettings={() => (showSettings = false)}
    onOpenRequest={() => void openPicked()}
    onState={(state) => (current = state)}
  >
    {#snippet empty()}
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
    {/snippet}
  </FileEditor>
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
