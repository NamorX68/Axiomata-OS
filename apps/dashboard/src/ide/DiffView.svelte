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
  import IconButton from "../ui/IconButton.svelte";

  import type { AgentFileChange, AgentState, IdeAgent } from "../core/backend";
  import { formatBytes } from "../core/format";
  import { toast } from "../core/toast";
  import type { DiffLayout } from "../editor/diff/view";
  import DiffPanes from "../fileapp/DiffPanes.svelte";
  import { KEEP_SCROLL } from "../fileapp/keepScroll";
  import { agentStatus, planTitle } from "./agentStatus";
  import { diffLayout, rememberDiffLayout, type DiffPlace } from "./diffPrefs";
  import { DiffRefresh } from "./diffRefresh";
  import { messageOf } from "../core/errors";
  import { AgentDiffSession } from "./diffSession.svelte";
  import GitActionDialog, { type GitAction } from "./GitActionDialog.svelte";

  interface Props {
    agent: IdeAgent;
    /** The agent's state from the status channel (CP6); `null` while unknown. */
    agentState: AgentState | null;
    /** Whether the view is on screen — hidden tabs do not poll (G4). */
    visible: boolean;
    place: DiffPlace;
    /** Open the agent's copy of `rel` at the zero-based `line` (H5, CP9). */
    onOpenFile?: (rel: string, line: number) => void;
    /** Shown first in the bar — in a dock pane, which agent this is (H14). */
    heading?: string;
    /** Move this view into a dock pane of its own (the side tab's "⧉ Dock"). */
    onDock?: () => void;
    /** How many files the agent changed, whenever that is known anew (the side bar's badge, editor-look I2). */
    onCount?: (files: number) => void;
  }

  let { agent, agentState, visible, place, onOpenFile, heading, onDock, onCount }: Props = $props();

  // An agent's view stays with its agent for as long as it is mounted.
  const session = new AgentDiffSession(untrack(() => agent.id));
  // `place` never changes for a mounted view (a tab stays a tab), so reading it once is right.
  let layout = $state<DiffLayout>(diffLayout(untrack(() => place)));
  let panes = $state<DiffPanes | null>(null);
  session.onLoaded = () => void tick().then(() => panes?.showFirstChange());
  $effect(() => onCount?.(session.files.length));

  /** The question being asked, and how answering it went (G3, G7, G13, H6). */
  let pending = $state<GitAction | null>(null);
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let conflictFiles = $state<string[] | null>(null);

  const statuses = agentStatus.statuses;
  const files = $derived(session.files);
  const base = $derived(session.base);
  const loaded = $derived(session.loaded);
  const model = $derived(session.model);
  const totals = $derived(
    files.reduce((t, f) => ({ add: t.add + (f.additions ?? 0), del: t.del + (f.deletions ?? 0) }), { add: 0, del: 0 }),
  );
  const working = $derived(agentState === "working");
  /** Why "Take over" is not possible right now (G12), or `null`. */
  const takeOverBlocked = $derived(
    agentState === "working" || agentState === "waiting"
      ? `Wait until ${agent.name} is idle`
      : session.hasUncommitted
        ? "Commit the uncommitted changes first"
        : files.length === 0
          ? "Nothing to take over"
          : null,
  );

  const refresher = new DiffRefresh(() => void session.refresh(false));
  $effect(() => refresher.update(visible, agentState));
  onDestroy(() => refresher.dispose());

  function toggleLayout(): void {
    layout = layout === "unified" ? "split" : "unified";
    rememberDiffLayout(place, layout);
  }

  function open(line: number): void {
    if (loaded && loaded.change.kind !== "deleted") onOpenFile?.(loaded.change.path, line);
  }

  function ask(action: GitAction): void {
    pending = action;
    problem = null;
    conflictFiles = null;
  }

  function askDiscardHunk(hunk: number): void {
    const header = loaded?.diff.hunks[hunk]?.header;
    if (loaded && header !== undefined) ask({ kind: "discard-hunk", path: loaded.change.path, hunk, header });
  }

  async function askTakeOver(): Promise<void> {
    if (takeOverBlocked || !base) return;
    const fromPlan = planTitle($statuses.byAgent.get(agent.id));
    const message = fromPlan ?? (await session.lastSubject().catch(() => null)) ?? "";
    ask({ kind: "take-over", message, mode: "squash", branch: base.branch });
  }

  /** Runs what the dialog confirmed; the dialog stays open with the reason if it fails. */
  async function run(action: GitAction): Promise<void> {
    busy = true;
    problem = null;
    try {
      if (action.kind === "discard-file") await session.discardFile();
      else if (action.kind === "discard-hunk") await session.discardHunk(action.hunk);
      else if (action.kind === "commit") {
        const commit = await session.commit(action.message);
        toast(`${agent.name}: committed ${commit.slice(0, 7)}`);
      } else {
        const result = await session.takeOver(action.mode, action.message);
        if (result.outcome === "conflict") {
          conflictFiles = result.files;
          return;
        }
        toast(`${agent.name}'s work is on ${action.branch} as ${result.commit.slice(0, 7)}`);
      }
      pending = null;
    } catch (err) {
      problem = messageOf(err);
    } finally {
      busy = false;
    }
  }

  /** ⌥⌘↓/⌥⌘↑ file, ⌘R reload, ⌘⇧D layout (H9) — wherever the focus is in the view. */
  function onKey(e: KeyboardEvent): boolean {
    const key = e.key.toLowerCase();
    if (e.metaKey && e.altKey && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      session.stepFile(e.key === "ArrowDown" ? 1 : -1);
      return true;
    }
    if (e.metaKey && !e.altKey && !e.shiftKey && key === "r") {
      void session.refresh(true);
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
    if (!e.defaultPrevented && !pending && onKey(e)) e.preventDefault();
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
  /** Git's header of every hunk of the open file, for the header rows and their Discard (H6). */
  const hunkHeaders = $derived(loaded ? { headers: loaded.diff.hunks.map((h) => h.header), discard: true } : null);
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="diffs" onkeydown={onKeydown}>
  <header class="bar">
    {#if heading}<span class="heading">{heading}</span>{/if}
    {#if base}
      <span class="base" title="Compared with the merge base {base.commit}">
        against <strong>{base.branch}</strong> @ {base.commit.slice(0, 7)}{base.fallback ? " (not recorded)" : ""}
      </span>
      <span class="totals"><span class="add">+{totals.add}</span> <span class="del">−{totals.del}</span></span>
    {/if}
    <span class="spacer"></span>
    {#if showsLines}
      <IconButton
        icon="file"
        size="sm"
        label="Show the whole file"
        pressed={session.wholeFile}
        onclick={() => session.toggleWholeFile()}
      />
      <IconButton
        icon={layout === "unified" ? "columns-2" : "rows-2"}
        size="sm"
        label={layout === "unified" ? "Two columns (⌘⇧D)" : "One column (⌘⇧D)"}
        onclick={toggleLayout}
      />
      {#if onOpenFile && loaded?.change.kind !== "deleted"}
        <IconButton icon="external-link" size="sm" label="Open the agent's copy (⏎)" onclick={() => open(panes?.openLine() ?? 0)} />
      {/if}
    {/if}
    {#if session.current}
      <button
        type="button"
        class="ax-btn danger"
        onclick={() => ask({ kind: "discard-file", path: session.current!.path })}
        title="Put this file back to how {base?.branch} has it">Discard file</button
      >
    {/if}
    {#if base}
      <button
        type="button"
        class="ax-btn"
        disabled={!session.hasUncommitted}
        onclick={() => ask({ kind: "commit", message: `wip: ${agent.name}` })}
        title={session.hasUncommitted ? "Commit what the agent left uncommitted" : "Nothing uncommitted"}
        >Commit…</button
      >
      <button
        type="button"
        class="ax-btn primary"
        disabled={takeOverBlocked !== null}
        onclick={() => void askTakeOver()}
        title={takeOverBlocked ?? `Take the committed work over into ${base.branch}`}>Take over…</button
      >
    {/if}
    {#if onDock}
      <IconButton icon="panel-right" size="sm" label="Open these diffs in a dock pane of their own" onclick={onDock} />
    {/if}
    <span class="reload" class:spinning={session.refreshing}>
      <IconButton icon="refresh-cw" size="sm" label="Reload (⌘R)" onclick={() => session.refresh(true)} />
    </span>
  </header>

  {#if session.listError}
    <p class="note error">{session.listError}</p>
  {:else if !session.diffState}
    <p class="note">Loading…</p>
  {:else if session.diffState.state === "shared_folder"}
    <p class="note">
      This project folder is not a git repository, so its agents work in it together and there is no diff per agent.
      Make it a git repository to give every agent a worktree of its own.
    </p>
  {:else if session.diffState.state === "not_started"}
    <p class="note">
      This agent has not been started yet. Its worktree and branch are made on the first start; what it changes shows up
      here.
    </p>
  {:else if files.length === 0}
    <p class="note">No changes against {base?.branch} yet.</p>
  {:else}
    <div class="body">
      <ul class="files" aria-label="Changed files" {...KEEP_SCROLL}>
        {#each files as file (file.path)}
          {@const part = splitPath(file.path)}
          <li>
            <button
              type="button"
              class="file"
              class:selected={file.path === session.selected}
              onclick={() => session.select(file)}
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
        {#if session.diffError}
          <p class="note error">{session.diffError}</p>
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
            revision={session.revision}
            oldText={loaded.oldText}
            newText={loaded.newText}
            {fileName}
            {hunkHeaders}
            onOpen={open}
            onDiscardHunk={askDiscardHunk}
            interceptKey={onKey}
          />
        {/if}
      </div>
    </div>
  {/if}

  {#if pending}
    <GitActionDialog
      action={pending}
      agentName={agent.name}
      {working}
      {busy}
      {problem}
      {conflictFiles}
      onConfirm={(action) => void run(action)}
      onCancel={() => (pending = null)}
    />
  {/if}
</div>

<style>
  .diffs {
    position: absolute;
    inset: 0;
    /* The body stacks list over diff when the pane is narrow (below). */
    container-type: inline-size;
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
    flex-wrap: wrap;
    align-items: center;
    gap: var(--ax-space-1) var(--ax-space-2);
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

  .heading {
    color: var(--ax-text);
  }

  /* The main actions are `.ax-btn` pills, the small ones icons (editor-look I6). */
  .reload {
    display: inline-flex;
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
    grid-template-columns: minmax(calc(160px * var(--ax-ui-scale)), 22%) 1fr;
    min-height: 0;
  }

  .files {
    margin: 0;
    padding: var(--ax-space-1) 0;
    list-style: none;
    overflow-y: auto;
    border-right: 1px solid var(--ax-border);
  }

  /* A narrow pane (a third of a split, a side tab): the list goes on top. */
  @container (max-width: 520px) {
    .body {
      grid-template-columns: 1fr;
      grid-template-rows: minmax(0, 30%) 1fr;
    }

    .files {
      border-right: none;
      border-bottom: 1px solid var(--ax-border);
    }
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
