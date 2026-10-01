<!--
  One file's change in the git panel, drawn on the editor (`docs/plans/editor-projekt-werkzeuge.md`,
  #48; the diff view of M7.3 `DiffPanes` — what a diff looks like is decided once). It lies over the
  editor area, which stays mounted underneath, so closing it brings every tab back as it was.

  Each hunk has its own button: *Stage* on the working tree's diff, *Unstage* on the staged one. A
  hunk is applied only if the file still reads as it was shown (the engine checks its header); a
  refusal is shown here and the diff is read again.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { messageOf } from "../core/errors";
  import { DiffModel } from "../editor/diff/model";
  import type { DiffLayout } from "../editor/diff/view";
  import IconButton from "../ui/IconButton.svelte";
  import DiffPanes from "./DiffPanes.svelte";
  import { gitApi, loadGitDiff, type GitEntry, type LoadedGitDiff, type Side } from "./gitBackend";

  interface Props {
    root: string;
    entry: Pick<GitEntry, "path" | "old_path">;
    side: Side;
    /** Bumped by the panel whenever its status was read again: the diff follows. */
    version?: number;
    onClose: () => void;
    /** After this view staged or unstaged something: the panel reads its status again. */
    onChanged: () => void;
    /** ⏎ on a line: open the working file there. */
    onOpenFile?: (line: number) => void;
  }

  let { root, entry, side, version = 0, onClose, onChanged, onOpenFile }: Props = $props();

  let loaded = $state.raw<LoadedGitDiff | null>(null);
  let model = $state.raw<DiffModel | null>(null);
  let revision = $state(0);
  let error = $state("");
  let layout = $state<DiffLayout>("unified");
  let busy = $state(false);
  let generation = 0;

  /** Reads the diff; a poll (`quiet`) that finds it unchanged leaves the view alone, so folds and the cursor stay. */
  async function load(quiet = false): Promise<void> {
    const mine = ++generation;
    try {
      const next = await loadGitDiff(root, entry, side);
      if (mine !== generation) return;
      if (quiet && loaded && JSON.stringify(loaded.diff) === JSON.stringify(next.diff)) return;
      loaded = next;
      model = new DiffModel(next.source);
      revision++;
      error = "";
    } catch (err) {
      if (mine === generation) error = messageOf(err);
    }
  }

  // A new file, side or status: read the diff again.
  $effect(() => {
    void root;
    void entry.path;
    void side;
    void version;
    untrack(() => void load());
  });

  // The file may change under the diff (an agent, a save): look again every few seconds.
  $effect(() => {
    const timer = setInterval(() => {
      if (!busy && document.visibilityState === "visible") void load(true);
    }, 5000);
    return () => clearInterval(timer);
  });

  const hunks = $derived(loaded?.diff.hunks ?? []);
  const headers = $derived({
    headers: hunks.map((h) => h.header),
    // A working-tree change can also be thrown away; a staged one is unstaged first.
    discard: side === "unstaged",
    action: side === "staged" ? ("unstage" as const) : ("stage" as const),
  });

  async function run(work: () => Promise<void>): Promise<void> {
    busy = true;
    try {
      await work();
      error = "";
    } catch (err) {
      error = messageOf(err);
    } finally {
      // Refused because the file changed meanwhile, or done: either way, what is there now.
      busy = false;
      onChanged();
      await load();
    }
  }

  const onHunk = (index: number, action: "stage" | "unstage") =>
    run(async () => {
      const header = hunks[index]?.header;
      if (header === undefined) return;
      if (action !== (side === "staged" ? "unstage" : "stage")) return;
      await gitApi.applyHunk(root, entry.path, entry.old_path, side, index, header);
    });

  const onDiscard = (index: number) =>
    run(async () => {
      const header = hunks[index]?.header;
      if (header === undefined || side !== "unstaged") return;
      if (!confirm("Throw this change away? It is not saved anywhere and cannot be brought back.")) return;
      await gitApi.discardHunk(root, entry.path, index, header);
    });

  const wholeFile = () =>
    run(() => (side === "staged" ? gitApi.unstage(root, [entry.path, ...(entry.old_path ? [entry.old_path] : [])]) : gitApi.stage(root, [entry.path])));
</script>

<section class="git-diff" aria-label="Change of {entry.path}">
  <header>
    <span class="title" title={entry.path}>{entry.path}</span>
    <span class="side">{side === "staged" ? "Staged" : "Changes"}</span>
    <span class="grow"></span>
    <button type="button" class="ax-btn" disabled={busy} onclick={() => void wholeFile()}>
      {side === "staged" ? "Unstage file" : "Stage file"}
    </button>
    <IconButton
      icon={layout === "unified" ? "columns-2" : "rows-2"}
      label={layout === "unified" ? "Show both sides next to each other" : "Show one column"}
      size="sm"
      onclick={() => (layout = layout === "unified" ? "split" : "unified")}
    />
    <IconButton icon="x" label="Close the change" size="sm" onclick={onClose} />
  </header>
  {#if error}<p class="problem" role="alert">{error}</p>{/if}
  {#if loaded && model}
    {#if loaded.diff.binary}
      <p class="note">Binary file — {loaded.diff.old_size ?? 0} → {loaded.diff.new_size ?? 0} bytes.</p>
    {:else if loaded.diff.truncated && hunks.length === 0}
      <p class="note">This file is too large to show as a diff.</p>
    {:else if hunks.length === 0}
      <p class="note">{side === "staged" ? "Nothing staged for this file." : "No unstaged changes in this file."}</p>
    {:else}
      <div class="panes">
        <DiffPanes
          {model}
          {layout}
          {revision}
          oldText={loaded.oldText}
          newText={loaded.newText}
          fileName={entry.path}
          hunkHeaders={headers}
          onStageHunk={(hunk, action) => void onHunk(hunk, action)}
          onDiscardHunk={(hunk) => void onDiscard(hunk)}
          onOpen={onOpenFile}
        />
      </div>
    {/if}
  {:else if !error}
    <p class="note">Reading the change…</p>
  {/if}
</section>

<style>
  .git-diff {
    position: absolute;
    inset: 0;
    z-index: 5;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--ax-bg);
  }

  header {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-3);
    border-bottom: 1px solid var(--ax-border);
    background: var(--ax-surface-1);
    font-size: var(--ax-font-size-sm);
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ax-text);
  }

  .side {
    padding: 0 var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .grow {
    flex: 1;
  }

  .panes {
    flex: 1;
    min-height: 0;
    display: flex;
  }

  .note,
  .problem {
    margin: 0;
    padding: var(--ax-space-3) var(--ax-space-4);
    font-size: var(--ax-font-size-sm);
  }

  .note {
    color: var(--ax-text-muted);
  }

  .problem {
    color: var(--ax-warning);
  }
</style>
