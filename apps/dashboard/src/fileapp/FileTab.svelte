<!--
  One tab of the file app (`docs/plans/editor.md`, ED4, W7): its own `FileEditor`,
  kept alive while the tab is open — hidden, not unmounted, when another tab is
  in front — so the cursor, undo, Vi's state and unsaved text stay with it.

  * **Loads what the tab names**, once: a file, a new note, or the live session
    a panel handed over (W11). A preview tab pointed at another file loads that.
  * **Reports where its editor went** (`onMoved`): a filed note, a followed
    link — so the tab follows instead of loading the old file again.
-->
<script lang="ts">
  import { untrack, type Snippet } from "svelte";

  import FileEditor, { type OpenFileState, type OpenResult } from "./FileEditor.svelte";
  import type { Handoff } from "./handoff";
  import type { FileRef } from "./tabs";

  interface Props {
    /** `null`: a new note. */
    file: FileRef | null;
    /** A panel's session to take over instead of opening the file (once). */
    handed?: Handoff["handed"];
    /** The line to put the cursor on when the file first opens (quick open's `:12`). */
    line?: number | null;
    visible: boolean;
    showSettings?: boolean;
    onCloseSettings?: () => void;
    onOpenRequest?: () => void;
    /** Vi's `:q`/`ZZ`: close this tab. */
    onQuit?: () => void;
    onState?: (state: OpenFileState | null) => void;
    /** The editor now shows another file than the tab named. */
    onMoved?: (file: FileRef) => void;
    /** The file could not be opened. */
    onFailed?: (result: Extract<OpenResult, { ok: false }>) => void;
    empty?: Snippet;
  }

  let {
    file,
    handed = null,
    line = null,
    visible,
    showSettings = false,
    onCloseSettings,
    onOpenRequest,
    onQuit,
    onState,
    onMoved,
    onFailed,
    empty,
  }: Props = $props();

  let editor = $state<FileEditor | null>(null);
  /** What the editor shows, as a key — what is loaded never loads again. */
  let loaded: string | null = null;
  /** The handed-over session is taken once; after that the tab loads files like any other. */
  let handoverTaken = false;

  const keyOf = (f: FileRef | null) => (f ? `${f.root}\0${f.rel}` : "new-note");

  $effect(() => {
    const target = file;
    const e = editor;
    if (!e || keyOf(target) === loaded) return;
    loaded = keyOf(target);
    untrack(() => void load(e, target));
  });

  async function load(e: FileEditor, target: FileRef | null): Promise<void> {
    if (handed && !handoverTaken) {
      handoverTaken = true;
      await e.adoptSession(handed);
      return;
    }
    if (target === null) {
      await e.newNote();
      return;
    }
    const result = await e.open(target, line);
    if (!result.ok) onFailed?.(result);
  }

  function reportState(state: OpenFileState | null): void {
    onState?.(state);
    if (!state || state.untitled) return;
    const now = { root: state.root, rel: state.rel };
    if (keyOf(now) === loaded) return;
    loaded = keyOf(now);
    onMoved?.(now);
  }

  export function hasUnsaved(): boolean {
    return editor?.hasUnsaved() ?? false;
  }

  export function saveNow(): Promise<boolean> {
    return editor?.saveNow() ?? Promise.resolve(true);
  }

  export function discard(): Promise<void> {
    return editor?.discard() ?? Promise.resolve();
  }

  /** Puts the cursor on `line` (zero-based) of the file this tab already shows. */
  export function goToLine(target: number): void {
    editor?.goToLine(target);
  }

  export function focus(): void {
    editor?.focus();
  }
</script>

<div class="tab-body" class:hidden={!visible} inert={!visible}>
  <FileEditor
    bind:this={editor}
    {visible}
    showSettings={showSettings && visible}
    {onCloseSettings}
    {onOpenRequest}
    {onQuit}
    onState={reportState}
    {empty}
  />
</div>

<style>
  /* Every tab fills the same place; the owner stacks them. */
  .tab-body {
    position: absolute;
    inset: 0;
    display: flex;
  }

  /* The tabs behind stay mounted and laid out (cursor, undo, unsaved text, scroll
     position — which `display: none` would lose) but out of sight and out of the way. */
  .tab-body.hidden {
    visibility: hidden;
    pointer-events: none;
  }
</style>
