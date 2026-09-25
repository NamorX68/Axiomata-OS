<!--
  The floating file panel (`docs/plans/editor.md`, ED4, W1–W4, W11): the editor
  where a file is opened *from somewhere* — the Second Brain, the chat's
  `/open`, an agent — and where a new note is written. It replaced the old
  Document viewer (`md-file`); every file kind it showed, it still shows
  (`FileEditor`: Markdown and HTML rendered, text and code, images).

  * **Config** `{ path, root?, mode, isNew }`: `path` is relative to `root`
    (the workspace when missing, W10); `mode` "read" opens Markdown and HTML
    rendered, "edit" on the source (W1); `isNew` without a path is a new note.
  * **The config follows the editor**: a filed note, or a link followed inside
    an HTML page, points the panel at its new file — so opening that file
    again brings this panel forward instead of a second one.
  * **Escape belongs to the editor** (D16); the panel closes with ⌘W or ×,
    and asks first when there is unsaved text (W11) or an unfiled note (W4).
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";

  import { requestClose, setCloseGuard } from "../core/staging";
  import type { ModuleContext } from "../core/types";
  import FileEditor, { type OpenFileState } from "./FileEditor.svelte";
  import type { OpenIntent } from "./fileKinds";
  import { panelTarget } from "./panelSync";

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per panel and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  let editor = $state<FileEditor | null>(null);
  let current = $state<OpenFileState | null>(null);
  let failure = $state<string | null>(null);
  /** The close question being asked; answering resolves the guard. */
  let asking = $state<((close: boolean) => void) | null>(null);

  const root = $derived(typeof $config.root === "string" && $config.root ? $config.root : "workspace");
  const path = $derived(typeof $config.path === "string" ? $config.path : "");
  const isNew = $derived($config.isNew === true && !path);
  const intent = $derived<OpenIntent>($config.mode === "edit" ? "edit" : "read");

  /** Opens what the config names; the editor keeps what it has if it is the same file. */
  $effect(() => {
    const target = { root, rel: path };
    const opening = { isNew, intent };
    const e = editor;
    if (!e) return;
    untrack(() => void load(e, target, opening));
  });

  async function load(
    e: FileEditor,
    target: { root: string; rel: string },
    opening: { isNew: boolean; intent: OpenIntent },
  ): Promise<void> {
    failure = null;
    if (opening.isNew) {
      await e.newNote();
      return;
    }
    if (!target.rel) return;
    const result = await e.open(target, null, opening.intent);
    if (!result.ok) failure = `Could not open ${target.rel}: ${result.message}`;
  }

  /** The editor moved to another file (a filed note, a followed link): the config follows. */
  function onState(state: OpenFileState | null): void {
    current = state;
    const patch = panelTarget($config, state);
    if (patch) config.update((old) => ({ ...old, ...patch }));
  }

  function ask(): Promise<boolean> {
    return new Promise((resolve) => {
      asking = (close) => {
        asking = null;
        resolve(close);
      };
    });
  }

  async function saveAndClose(): Promise<void> {
    const answer = asking;
    if (!answer || !editor) return;
    if (await editor.saveNow()) answer(true);
  }

  async function discardAndClose(): Promise<void> {
    const answer = asking;
    if (!answer) return;
    await editor?.discard();
    answer(true);
  }

  function onKeydown(e: KeyboardEvent): void {
    // D16: Escape is the editor's (Vi's Normal mode, a completion …), never the panel's.
    if (e.key === "Escape") {
      e.preventDefault();
      return;
    }
    if (e.metaKey && !e.altKey && !e.ctrlKey && !e.shiftKey && e.key.toLowerCase() === "w") {
      e.preventDefault();
      void requestClose(ctx.instanceId);
    }
  }

  onMount(() => {
    setCloseGuard(ctx.instanceId, async () => (editor?.hasUnsaved() ? ask() : true));
    return () => setCloseGuard(ctx.instanceId, null);
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="file-panel" onkeydown={onKeydown}>
  <div class="title">
    <span class="name">
      {#if current?.untitled}
        New note
      {:else if current}
        {current.rel}
      {:else}
        {path}
      {/if}
    </span>
    {#if current?.dirty}<span class="dot" title="Unsaved changes"></span>{/if}
  </div>

  {#if asking}
    <div class="ask" role="alertdialog" aria-label="Unsaved changes">
      {#if current?.untitled}
        <span>This note is not filed yet.</span>
        <button type="button" class="strong" onclick={() => void saveAndClose()}>File it</button>
        <button type="button" onclick={() => void discardAndClose()}>Discard</button>
      {:else}
        <span>{current?.rel ?? "This file"} has unsaved changes.</span>
        <button type="button" class="strong" onclick={() => void saveAndClose()}>Save</button>
        <button type="button" onclick={() => void discardAndClose()}>Discard</button>
      {/if}
      <button type="button" onclick={() => asking?.(false)}>Cancel</button>
    </div>
  {/if}

  {#if failure}
    <p class="failure" role="alert">{failure}</p>
  {/if}

  <FileEditor bind:this={editor} {onState} onQuit={() => void requestClose(ctx.instanceId)} />
</div>

<style>
  .file-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .title {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-5);
    border-bottom: 1px solid var(--ax-border);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    font-family: var(--ax-font-mono);
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dot {
    width: var(--ax-space-2);
    height: var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-accent);
    flex-shrink: 0;
  }

  .ask {
    display: flex;
    align-items: center;
    gap: var(--ax-space-3);
    padding: var(--ax-space-2) var(--ax-space-5);
    background: var(--ax-accent-muted);
    border-bottom: 1px solid var(--ax-border);
    font-size: var(--ax-font-size-sm);
  }

  .ask span {
    flex: 1;
  }

  .ask button {
    padding: var(--ax-space-1) var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
    cursor: pointer;
  }

  .ask button.strong {
    border-color: var(--ax-accent);
    color: var(--ax-accent);
  }

  .failure {
    margin: 0;
    padding: var(--ax-space-2) var(--ax-space-5);
    color: var(--ax-danger);
    font-size: var(--ax-font-size-sm);
  }
</style>
