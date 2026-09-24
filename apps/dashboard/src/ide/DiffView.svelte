<!--
  What an agent has changed (`docs/plans/git-layer.md`, CP8): the changed files
  on the left, the chosen file's diff on the right, drawn on the editor
  (`fileapp/DiffPanes.svelte`, H1–H4). Lives in an agent's side tab and, from
  CP9, in a dock pane of its own (H14) — `place` says which, for the layout it
  remembers.

  * **Reloads by G4** (`diffRefresh.ts`): coming into view, the agent stopping,
    every 5 s while in view and working; ↻ / ⌘R any time. A reload keeps the
    open file's folds when its hunks did not change — polling must not snap the
    view back while it is being read.
  * **Always says why it is empty**: a shared folder has no diff per agent, an
    agent never started has no worktree yet, a fresh worktree has no changes.
-->
<script lang="ts">
  import { onDestroy, tick, untrack } from "svelte";

  import type { AgentDiffState, AgentFileChange, AgentState, IdeAgent } from "../core/backend";
  import { formatBytes } from "../core/format";
  import { DiffModel } from "../editor/diff/model";
  import type { DiffLayout } from "../editor/diff/view";
  import DiffPanes from "../fileapp/DiffPanes.svelte";
  import { diffLayout, rememberDiffLayout, type DiffPlace } from "./diffPrefs";
  import { DiffRefresh } from "./diffRefresh";
  import { agentChanges, agentFileDiff, loadFileDiff, sameFileDiff, type LoadedDiff } from "./git";

  interface Props {
    agent: IdeAgent;
    /** The agent's state from the status channel (CP6); `null` while unknown. */
    agentState: AgentState | null;
    /** Whether the view is on screen — hidden tabs do not poll (G4). */
    visible: boolean;
    place: DiffPlace;
    /** Open the agent's copy of `rel` at the zero-based `line` (H5, CP9). */
    onOpenFile?: (rel: string, line: number) => void;
  }

  let { agent, agentState, visible, place, onOpenFile }: Props = $props();

  let diffState = $state.raw<AgentDiffState | null>(null);
  let listError = $state("");
  let selected = $state<string | null>(null);
  let loaded = $state.raw<LoadedDiff | null>(null);
  let model = $state.raw<DiffModel | null>(null);
  let diffError = $state("");
  /** Bumped when the model changed outside `DiffPanes` (a new file, whole file on/off). */
  let revision = $state(0);
  let wholeFile = $state(false);
  // `place` never changes for a mounted view (a tab stays a tab), so reading it once is right.
  let layout = $state<DiffLayout>(diffLayout(untrack(() => place)));
  let refreshing = $state(false);
  let panes = $state<DiffPanes | null>(null);
  /** Guards against an older request answering after a newer one. */
  let listGeneration = 0;
  let fileGeneration = 0;

  const files = $derived(diffState?.state === "ready" ? diffState.changes.files : []);
  const base = $derived(diffState?.state === "ready" ? diffState.changes.base : null);
  const totals = $derived(
    files.reduce((t, f) => ({ add: t.add + (f.additions ?? 0), del: t.del + (f.deletions ?? 0) }), { add: 0, del: 0 }),
  );

  const refresher = new DiffRefresh(() => void refresh(false));
  $effect(() => refresher.update(visible, agentState));
  onDestroy(() => refresher.dispose());

  /** Reloads the list, then the open file if it changed (or always, when `force`). */
  export async function refresh(force: boolean): Promise<void> {
    const mine = ++listGeneration;
    refreshing = true;
    try {
      const next = await agentChanges(agent.id);
      if (mine !== listGeneration) return;
      diffState = next;
      listError = "";
    } catch (err) {
      if (mine === listGeneration) listError = messageOf(err);
      return;
    } finally {
      if (mine === listGeneration) refreshing = false;
    }
    const current = files.find((f) => f.path === selected);
    if (current) await reloadIfChanged(current, force);
    else await select(files[0] ?? null);
  }

  /** Reloads the open file, unless its diff is exactly what is shown already. */
  async function reloadIfChanged(change: AgentFileChange, force: boolean): Promise<void> {
    if (!force && loaded && loaded.change.path === change.path) {
      try {
        const diff = await agentFileDiff(agent.id, change.path, change.old_path);
        // Another file was picked meanwhile: that pick has its own load.
        if (selected !== change.path) return;
        if (sameFileDiff(diff, loaded.diff)) return;
      } catch {
        // Fall through to a full load, which reports the error.
      }
    }
    await load(change);
  }

  async function select(change: AgentFileChange | null): Promise<void> {
    selected = change?.path ?? null;
    if (change) await load(change);
    else {
      loaded = null;
      model = null;
    }
  }

  async function load(change: AgentFileChange): Promise<void> {
    const mine = ++fileGeneration;
    diffError = "";
    try {
      const next = await loadFileDiff(agent.id, change);
      // A newer load, or a poll that started before another file was picked.
      if (mine !== fileGeneration || selected !== change.path) return;
      const nextModel = new DiffModel(next.source);
      if (wholeFile) nextModel.setWholeFile(true);
      loaded = next;
      model = nextModel;
      revision++;
      await tick();
      panes?.showFirstChange();
    } catch (err) {
      if (mine === fileGeneration) diffError = messageOf(err);
    }
  }

  function messageOf(err: unknown): string {
    if (typeof err === "string") return err;
    if (err instanceof Error) return err.message;
    return (err as { message?: string })?.message ?? String(err);
  }

  function stepFile(step: 1 | -1): void {
    if (files.length === 0) return;
    const at = files.findIndex((f) => f.path === selected);
    const next = files[Math.max(0, Math.min(files.length - 1, at + step))];
    if (next && next.path !== selected) void select(next);
  }

  function toggleLayout(): void {
    layout = layout === "unified" ? "split" : "unified";
    rememberDiffLayout(place, layout);
  }

  function toggleWholeFile(): void {
    wholeFile = !wholeFile;
    model?.setWholeFile(wholeFile);
    revision++;
  }

  function open(line: number): void {
    if (loaded && loaded.change.kind !== "deleted") onOpenFile?.(loaded.change.path, line);
  }

  /** ⌥⌘↓/⌥⌘↑ file, ⌘R reload, ⌘⇧D layout (H9) — wherever the focus is in the view. */
  function onKey(e: KeyboardEvent): boolean {
    const key = e.key.toLowerCase();
    if (e.metaKey && e.altKey && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      stepFile(e.key === "ArrowDown" ? 1 : -1);
      return true;
    }
    if (e.metaKey && !e.altKey && !e.shiftKey && key === "r") {
      void refresh(true);
      return true;
    }
    if (e.metaKey && e.shiftKey && key === "d") {
      toggleLayout();
      return true;
    }
    return false;
  }

  function onKeydown(e: KeyboardEvent): void {
    // The diff's own surface already had its turn (`interceptKey`) and marked it.
    if (!e.defaultPrevented && onKey(e)) e.preventDefault();
  }

  const KIND_MARK: Record<AgentFileChange["kind"], { mark: string; title: string }> = {
    added: { mark: "A", title: "Added" },
    modified: { mark: "M", title: "Modified" },
    deleted: { mark: "D", title: "Deleted" },
    renamed: { mark: "R", title: "Renamed" },
    type_changed: { mark: "T", title: "Became a link, or stopped being one" },
  };

  function splitPath(path: string): { dir: string; name: string } {
    const slash = path.lastIndexOf("/");
    return slash < 0 ? { dir: "", name: path } : { dir: path.slice(0, slash + 1), name: path.slice(slash + 1) };
  }

  const fileName = $derived(loaded ? splitPath(loaded.change.path).name : "");
  /** The open file is shown as lines (not a picture, a binary note or an empty diff). */
  const showsLines = $derived(
    !!loaded && !!model && !loaded.images && !loaded.diff.binary && !loaded.change.binary && model.hunkCount > 0,
  );
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="diffs" onkeydown={onKeydown}>
  <header class="bar">
    {#if base}
      <span class="base" title="Compared with the merge base {base.commit}">
        against <strong>{base.branch}</strong> @ {base.commit.slice(0, 7)}{base.fallback ? " (not recorded)" : ""}
      </span>
      <span class="totals"><span class="add">+{totals.add}</span> <span class="del">−{totals.del}</span></span>
    {/if}
    <span class="spacer"></span>
    {#if showsLines}
      <button type="button" class:on={wholeFile} onclick={toggleWholeFile} title="Show the whole file">Whole file</button>
      <button type="button" onclick={toggleLayout} title="One column or two (⌘⇧D)">
        {layout === "unified" ? "Split" : "Unified"}
      </button>
      {#if onOpenFile && loaded?.change.kind !== "deleted"}
        <button type="button" onclick={() => open(panes?.openLine() ?? 0)} title="Open the agent's copy (⏎)">Open</button>
      {/if}
    {/if}
    <button type="button" class="reload" class:spinning={refreshing} onclick={() => refresh(true)} title="Reload (⌘R)"
      >↻</button
    >
  </header>

  {#if listError}
    <p class="note error">{listError}</p>
  {:else if !diffState}
    <p class="note">Loading…</p>
  {:else if diffState.state === "shared_folder"}
    <p class="note">
      This project folder is not a git repository, so its agents work in it together and there is no diff per agent.
      Make it a git repository to give every agent a worktree of its own.
    </p>
  {:else if diffState.state === "not_started"}
    <p class="note">
      This agent has not been started yet. Its worktree and branch are made on the first start; what it changes shows up
      here.
    </p>
  {:else if files.length === 0}
    <p class="note">No changes against {base?.branch} yet.</p>
  {:else}
    <div class="body">
      <ul class="files" aria-label="Changed files">
        {#each files as file (file.path)}
          {@const part = splitPath(file.path)}
          <li>
            <button
              type="button"
              class="file"
              class:selected={file.path === selected}
              onclick={() => select(file)}
              title={file.old_path ? `${file.old_path} → ${file.path}` : file.path}
            >
              <span class="kind {file.kind}" title={KIND_MARK[file.kind].title}>{KIND_MARK[file.kind].mark}</span>
              <span class="path"><span class="name">{part.name}</span> <span class="dir">{part.dir}</span></span>
              {#if file.uncommitted}<span class="open" title="Not committed yet">●</span>{/if}
              {#if file.additions === null}
                <span class="counts">bin</span>
              {:else}
                <span class="counts"><span class="add">+{file.additions}</span> <span class="del">−{file.deletions}</span></span>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
      <div class="diff">
        {#if diffError}
          <p class="note error">{diffError}</p>
        {:else if !loaded || !model}
          <p class="note">Loading…</p>
        {:else if loaded.images}
          <div class="images">
            <figure>
              {#if loaded.images.old}<img src={loaded.images.old} alt="Before" />{:else}<div class="none">none</div>{/if}
              <figcaption>Before{loaded.diff.old_size !== null ? ` · ${formatBytes(loaded.diff.old_size)}` : ""}</figcaption>
            </figure>
            <figure>
              {#if loaded.images.new}<img src={loaded.images.new} alt="After" />{:else}<div class="none">none</div>{/if}
              <figcaption>After{loaded.diff.new_size !== null ? ` · ${formatBytes(loaded.diff.new_size)}` : ""}</figcaption>
            </figure>
          </div>
        {:else if loaded.diff.binary || loaded.change.binary}
          <p class="note">
            Binary file changed: {loaded.diff.old_size === null ? "new" : formatBytes(loaded.diff.old_size)} →
            {loaded.diff.new_size === null ? "deleted" : formatBytes(loaded.diff.new_size)}.
          </p>
        {:else if model.hunkCount === 0}
          <p class="note">
            {loaded.diff.truncated
              ? "Too large to show as a diff."
              : loaded.change.kind === "renamed"
                ? `Renamed from ${loaded.change.old_path}, content unchanged.`
                : "No line changes (only the file's mode or type)."}
          </p>
        {:else}
          <DiffPanes
            bind:this={panes}
            {model}
            {layout}
            {revision}
            oldText={loaded.oldText}
            newText={loaded.newText}
            {fileName}
            onOpen={open}
            interceptKey={onKey}
          />
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .diffs {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    background: var(--ax-surface-1);
    color: var(--ax-text);
    font-size: var(--ax-font-size-sm);
  }

  .bar {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-2);
    border-bottom: 1px solid var(--ax-border);
    color: var(--ax-text-muted);
    white-space: nowrap;
  }

  .bar strong {
    color: var(--ax-text);
    font-weight: normal;
  }

  .spacer {
    flex: 1;
  }

  .bar button {
    padding: 0 var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    color: var(--ax-text);
    font: inherit;
    cursor: pointer;
  }

  .bar button:hover,
  .bar button.on {
    border-color: var(--ax-accent);
    color: var(--ax-accent);
  }

  .reload.spinning {
    opacity: 0.5;
  }

  .add {
    color: var(--ax-success);
  }

  .del {
    color: var(--ax-danger);
  }

  .note {
    margin: 0;
    padding: var(--ax-space-4);
    color: var(--ax-text-muted);
  }

  .note.error {
    color: var(--ax-danger);
  }

  .body {
    flex: 1;
    display: grid;
    grid-template-columns: minmax(160px, 22%) 1fr;
    min-height: 0;
  }

  .files {
    margin: 0;
    padding: var(--ax-space-1) 0;
    list-style: none;
    overflow-y: auto;
    border-right: 1px solid var(--ax-border);
  }

  .file {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    width: 100%;
    padding: var(--ax-space-1) var(--ax-space-2);
    border: none;
    background: none;
    color: var(--ax-text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .file:hover {
    background: var(--ax-surface-2);
  }

  .file.selected {
    background: var(--ax-accent-muted);
  }

  .kind {
    flex: none;
    width: 1.4em;
    border-radius: var(--ax-radius-sm);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    text-align: center;
  }

  .kind.added {
    color: var(--ax-success);
  }

  .kind.deleted {
    color: var(--ax-danger);
  }

  .kind.modified,
  .kind.renamed,
  .kind.type_changed {
    color: var(--ax-warning);
  }

  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dir {
    color: var(--ax-text-muted);
  }

  .open {
    color: var(--ax-accent);
    font-size: var(--ax-font-size-xs);
  }

  .counts {
    flex: none;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    color: var(--ax-text-muted);
  }

  .diff {
    position: relative;
    min-width: 0;
    min-height: 0;
  }

  .images {
    display: flex;
    gap: var(--ax-space-4);
    padding: var(--ax-space-4);
    overflow: auto;
  }

  .images figure {
    margin: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--ax-space-2);
  }

  .images img {
    max-width: 100%;
    max-height: 60vh;
    border: 1px solid var(--ax-border);
    image-rendering: auto;
  }

  .images .none {
    padding: var(--ax-space-4);
    border: 1px dashed var(--ax-border);
    color: var(--ax-text-muted);
  }

  .images figcaption {
    color: var(--ax-text-muted);
  }
</style>
