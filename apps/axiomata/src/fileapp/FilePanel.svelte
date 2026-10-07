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
  * **Escape belongs to the editor** (D16) while the source is shown; on a rendered page or a picture it closes the
    panel. ⌘W or × close it always,
    and it asks first when there is unsaved text (W11) or an unfiled note (W4).
-->
<script lang="ts">
  import { onMount, tick, untrack } from "svelte";

  import { closeStaged, requestClose, setCloseGuard } from "../core/staging";
  import type { ModuleContext } from "../core/types";
  import {
    DEFAULT_EDITOR_SETTINGS,
    editorSettings,
    PANEL_FONT_SIZE_MAX,
    PANEL_FONT_SIZE_MIN,
    PANEL_FONT_SIZE_STEP,
    updateEditorSettings,
  } from "./editorSettings";
  import FileEditor, { type OpenFileState } from "./FileEditor.svelte";
  import { applyPanelZoomStep, panelZoomStep, type PanelZoomStep } from "./panelZoomKeys";
  import type { OpenIntent } from "./fileKinds";
  import { handToFileApp } from "./handoff";
  import { panelTarget } from "./panelSync";
  import UnsavedQuestion from "./UnsavedQuestion.svelte";

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

  /**
   * "Open in the file app" (W11): the file moves into a tab there, unsaved
   * text and undo included, and this panel closes — nothing is left to ask about.
   */
  function handOver(): void {
    const state = current;
    if (!state) return;
    const handed = editor?.detach() ?? null;
    handToFileApp({ file: state.untitled ? null : { root: state.root, rel: state.rel }, handed });
    closeStaged(ctx.instanceId);
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

  /** One text-size step, from the header's controls or the keys. */
  function zoom(step: PanelZoomStep): void {
    updateEditorSettings({ panelFontSize: applyPanelZoomStep($editorSettings.panelFontSize, step) });
  }

  function onKeydown(e: KeyboardEvent): void {
    // D16: Escape is the editor's (Vi's Normal mode, a completion …) — except on a rendered page or a picture, where
    // the editor has no use for it and a bare Escape closes the window like the Kanban's.
    if (e.key === "Escape") {
      e.preventDefault();
      const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
      if (!typing && editor?.isReadOnlyView()) void requestClose(ctx.instanceId);
      return;
    }
    // In the editor the zoom keys are taken (and stopped) there; this is for focus in the header's controls.
    const step = panelZoomStep(e);
    if (step) {
      e.preventDefault();
      zoom(step);
      return;
    }
    if (e.metaKey && !e.altKey && !e.ctrlKey && !e.shiftKey && e.key.toLowerCase() === "w") {
      e.preventDefault();
      void requestClose(ctx.instanceId);
    }
  }

  /** The panel's own element: the zoom keys and ⌘W are heard only while focus is inside it. */
  let panel = $state<HTMLDivElement | null>(null);

  onMount(() => {
    // Opened from the Second Brain or the Orbit, focus would stay where the click was and the keys of this window would
    // never reach it (a rendered page has nothing to focus). The editor takes it for itself when it is to be typed in.
    void tick().then(() => {
      if (panel && !panel.contains(document.activeElement)) panel.focus({ preventScroll: true });
    });
    setCloseGuard(ctx.instanceId, async () => (editor?.hasUnsaved() ? ask() : true));
    return () => setCloseGuard(ctx.instanceId, null);
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="file-panel" bind:this={panel} tabindex="-1" onkeydown={onKeydown}>
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
    <span class="spacer"></span>
    <div class="zoom" role="group" aria-label="Text size">
      <button
        type="button"
        class="zoom-step"
        aria-label="Smaller text"
        title="Smaller text (⌘-)"
        disabled={$editorSettings.panelFontSize <= PANEL_FONT_SIZE_MIN}
        onclick={() => zoom("down")}>A−</button
      >
      <input
        type="range"
        class="zoom-range"
        aria-label="Text size"
        min={PANEL_FONT_SIZE_MIN}
        max={PANEL_FONT_SIZE_MAX}
        step={PANEL_FONT_SIZE_STEP}
        value={$editorSettings.panelFontSize}
        oninput={(e) => updateEditorSettings({ panelFontSize: e.currentTarget.valueAsNumber })}
      />
      <button
        type="button"
        class="zoom-step"
        aria-label="Larger text"
        title="Larger text (⌘+)"
        disabled={$editorSettings.panelFontSize >= PANEL_FONT_SIZE_MAX}
        onclick={() => zoom("up")}>A+</button
      >
      <span class="zoom-size" aria-live="polite">{$editorSettings.panelFontSize}</span>
      <button
        type="button"
        class="zoom-step"
        aria-label="Reset text size"
        title="Reset text size (⌘0)"
        disabled={$editorSettings.panelFontSize === DEFAULT_EDITOR_SETTINGS.panelFontSize}
        onclick={() => zoom("reset")}>Reset</button
      >
    </div>
    <button type="button" class="hand-over" disabled={!current} onclick={handOver}>Open in the file app</button>
  </div>

  {#if asking}
    <UnsavedQuestion
      name={current?.rel ?? "This file"}
      untitled={current?.untitled ?? false}
      onSave={() => void saveAndClose()}
      onDiscard={() => void discardAndClose()}
      onCancel={() => asking?.(false)}
    />
  {/if}

  {#if failure}
    <p class="failure" role="alert">{failure}</p>
  {/if}

  <FileEditor bind:this={editor} compact {onState} onQuit={() => void requestClose(ctx.instanceId)} />
</div>

<style>
  .file-panel {
    outline: none;
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

  .spacer {
    flex: 1;
  }

  .zoom {
    display: flex;
    align-items: center;
    gap: var(--ax-space-1);
  }

  .zoom-step {
    padding: 0 var(--ax-space-2);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
    cursor: pointer;
  }

  .zoom-step:hover:not(:disabled) {
    border-color: var(--ax-accent);
    color: var(--ax-text);
  }

  .zoom-step:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .zoom-range {
    width: calc(80px * var(--ax-ui-scale));
    accent-color: var(--ax-accent);
  }

  /* A fixed width, so the header does not shift while the number changes. */
  .zoom-size {
    min-width: 2ch;
    text-align: right;
    color: var(--ax-text);
  }

  .hand-over {
    padding: 0 var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
    cursor: pointer;
  }

  .hand-over:hover:not(:disabled) {
    border-color: var(--ax-accent);
    color: var(--ax-text);
  }

  .dot {
    width: var(--ax-space-2);
    height: var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-accent);
    flex-shrink: 0;
  }

  .failure {
    margin: 0;
    padding: var(--ax-space-2) var(--ax-space-5);
    color: var(--ax-danger);
    font-size: var(--ax-font-size-sm);
  }
</style>
