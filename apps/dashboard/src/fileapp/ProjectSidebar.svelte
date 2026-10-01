<!--
  The sidebar of the workbench (`docs/plans/workbench.md`, step 1): the project bar, the tabs
  *Files | Search | Git*, the file tree with the outline under it, and the edge to drag for its width.
  One component for the file app and the IDE, so both look and behave the same.

  The host owns what is specific to it and hands it in:

  * **the project** — which one is open and what the project bar's actions do;
  * **the files** — the roots the tree shows, what opening a file means in its dock, the file in front;
  * **the outline** — the front file's symbols and where its cursor is;
  * **the git change** — the sidebar lists changes, but the host draws the diff over its own dock.

  The sidebar owns its tab, its width, the folders open in the tree and the outline's height
  (`prefs`, saved by the host under its own key), and the two things only it can do: focus the
  search field (`showSearch`) and show a language server's list of places (`showLocations`).
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import type { FileRootInfo, IdeProject } from "../core/backend";
  import { uiScale } from "../core/uiScale";
  import IconButton from "../ui/IconButton.svelte";
  import FileTree from "./FileTree.svelte";
  import GitPanel from "./GitPanel.svelte";
  import type { RepoStatus, Side } from "./gitBackend";
  import type { GitRow } from "./gitModel";
  import type { LocationList } from "./locationList";
  import OutlinePanel from "./OutlinePanel.svelte";
  import type { OutlineInfo } from "./outlineModel";
  import ProjectBar from "./ProjectBar.svelte";
  import ProjectSearch from "./ProjectSearch.svelte";
  import type { FileRef } from "./tabs";
  import { clampOutlineHeight, clampWidth, type TreePrefs } from "./treeModel";

  interface Props {
    /** Width, open folders, hidden-file toggle, outline fold and height — saved by the host. */
    prefs: TreePrefs;
    /** The view is on screen (the git panel keeps itself up to date only then). */
    open: boolean;

    // The project bar.
    projects: IdeProject[];
    current: IdeProject | null;
    busy?: boolean;
    onPickProject: (id: number) => void;
    onOpenFolder: () => void;
    onNewProject: (name: string, gitInit: boolean) => void;
    onCloseProject: () => void;
    onChangeFolder: (id: number) => void;
    onRemoveProject: (id: number) => void;

    // The tree and the search.
    roots: FileRootInfo[];
    /** The file in front of the dock, highlighted in the tree. */
    active: FileRef | null;
    onOpenFile: (file: FileRef, preview: boolean) => void;
    /** A search result: the file and the line. */
    onOpenResult: (root: string, rel: string, line: number) => void;
    onError: (message: string) => void;

    // The outline.
    outline: OutlineInfo | null;
    onJumpToSymbol: (line: number) => void;

    // The git panel.
    gitRoot: string | null;
    gitSelected: { path: string; side: Side } | null;
    /** Bumped from outside (the open change did something): the status is read again. */
    gitRefresh?: number;
    onOpenChange: (row: GitRow) => void;
    onGitStatus?: (changes: number, status: RepoStatus | null) => void;

    /** The Agents view, which the host owns (it needs the project's agents and the dock). */
    agentsView?: Snippet;
  }

  let {
    prefs = $bindable(),
    open,
    projects,
    current,
    busy = false,
    onPickProject,
    onOpenFolder,
    onNewProject,
    onCloseProject,
    onChangeFolder,
    onRemoveProject,
    roots,
    active,
    onOpenFile,
    onOpenResult,
    onError,
    outline,
    onJumpToSymbol,
    gitRoot,
    gitSelected,
    gitRefresh = 0,
    onOpenChange,
    onGitStatus,
    agentsView,
  }: Props = $props();

  /** Which view the column shows — chosen by the activity rail, kept in the prefs. */
  const tab = $derived(prefs.view);
  const setTab = (view: TreePrefs["view"]): void => {
    prefs = { ...prefs, view };
  };
  let treeView = $state<FileTree | null>(null);
  let searchView = $state<ProjectSearch | null>(null);
  /** The edge being dragged for the width, and the outline's top edge for its height. */
  let widthDrag = $state<{ startX: number; startWidth: number } | null>(null);
  let outlineDrag = $state<{ startY: number; startHeight: number } | null>(null);

  /** Whether a drag is in progress — the host saves the prefs once the edge is let go, not on every pixel. */
  export function dragging(): boolean {
    return widthDrag !== null || outlineDrag !== null;
  }

  export function showFiles(): void {
    setTab("files");
  }

  /** ⇧⌘F: the search tab, its field focused. */
  export async function showSearch(): Promise<void> {
    setTab("search");
    await searchView?.focus();
  }

  /** A language server's list of places (ED6.3) in the search tab. */
  export function showLocations(list: LocationList): void {
    setTab("search");
    searchView?.showLocations(list);
  }

  export function refreshTree(): void {
    treeView?.refresh();
  }

  function startWidthDrag(e: PointerEvent): void {
    widthDrag = { startX: e.clientX, startWidth: prefs.width };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onWidthDrag(e: PointerEvent): void {
    // The width is unscaled, drawn times the UI scale (editor-look K10); the pointer moves in screen pixels.
    if (widthDrag) prefs = { ...prefs, width: clampWidth(widthDrag.startWidth + (e.clientX - widthDrag.startX) / $uiScale) };
  }

  function startOutlineDrag(e: PointerEvent): void {
    outlineDrag = { startY: e.clientY, startHeight: prefs.outlineHeight };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onOutlineDrag(e: PointerEvent): void {
    // Unscaled like the width; up makes it taller.
    if (outlineDrag) {
      prefs = {
        ...prefs,
        outlineHeight: clampOutlineHeight(outlineDrag.startHeight - (e.clientY - outlineDrag.startY) / $uiScale),
      };
    }
  }
</script>

<aside class="side" style:width="{prefs.width * $uiScale}px">
  <ProjectBar
    {projects}
    {current}
    {busy}
    onPick={onPickProject}
    {onOpenFolder}
    onNew={onNewProject}
    onClose={onCloseProject}
    {onChangeFolder}
    onRemove={onRemoveProject}
  />
  {#if tab === "files"}
    <div class="side-bar">
      <span class="side-title">FILES</span>
      <IconButton
        icon={prefs.showHidden ? "eye" : "eye-off"}
        label={prefs.showHidden ? "Hide dotfiles, .git, node_modules, target" : "Show dotfiles, .git, node_modules, target"}
        pressed={prefs.showHidden}
        size="sm"
        onclick={() => (prefs = { ...prefs, showHidden: !prefs.showHidden })}
      />
      <IconButton icon="refresh-cw" label="Read the folders again" size="sm" onclick={() => treeView?.refresh()} />
    </div>
  {/if}
  <!-- All the views stay mounted: the tree keeps what is open, the search its results. -->
  <div class="side-pane" class:gone={tab !== "search"}>
    <ProjectSearch
      bind:this={searchView}
      {roots}
      initialRoot={active?.root ?? null}
      onOpen={onOpenResult}
    />
  </div>
  <div class="side-pane" class:gone={tab !== "git"}>
    <GitPanel
      root={gitRoot}
      active={open && tab === "git"}
      selected={gitSelected}
      refresh={gitRefresh}
      onOpen={onOpenChange}
      onStatus={(count, status) => {
        onGitStatus?.(count, status);
      }}
    />
  </div>
  <div class="side-pane" class:gone={tab !== "agents"}>{@render agentsView?.()}</div>
  <div class="side-pane" class:gone={tab !== "files"}>
    <div class="tree-area">
      <FileTree
        bind:this={treeView}
        {roots}
        bind:expanded={prefs.expanded}
        showHidden={prefs.showHidden}
        {active}
        onOpen={onOpenFile}
        {onError}
      />
      {#if roots.length === 0}
        <p class="no-project">
          {current ? "The project folder is not available." : "No project open. Open a folder or start a new project from the bar above."}
        </p>
      {/if}
    </div>
    {#if prefs.outlineOpen}
      <div
        class="outline-edge"
        role="separator"
        aria-orientation="horizontal"
        aria-label="Outline height"
        onpointerdown={startOutlineDrag}
        onpointermove={onOutlineDrag}
        onpointerup={() => (outlineDrag = null)}
      ></div>
    {/if}
    <div class="outline-area" style:height={prefs.outlineOpen ? `${prefs.outlineHeight * $uiScale}px` : "auto"}>
      <OutlinePanel
        info={outline}
        open={prefs.outlineOpen}
        onToggle={() => (prefs = { ...prefs, outlineOpen: !prefs.outlineOpen })}
        onJump={onJumpToSymbol}
      />
    </div>
  </div>
</aside>
<div
  class="edge"
  class:dragging={widthDrag !== null}
  role="separator"
  aria-orientation="vertical"
  aria-label="Sidebar width"
  onpointerdown={startWidthDrag}
  onpointermove={onWidthDrag}
  onpointerup={() => (widthDrag = null)}
></div>

<style>
  .side {
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    min-height: 0;
    background: var(--ax-surface-1);
  }

  .side-bar {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-3);
    border-bottom: 1px solid var(--ax-border);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .side-title {
    flex: 1;
    letter-spacing: var(--ax-tracking-wide);
  }

  .side-pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .side-pane.gone {
    display: none;
  }

  .tree-area {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }

  .outline-area {
    flex-shrink: 0;
    min-height: 0;
  }

  .outline-edge {
    height: var(--ax-space-1);
    flex-shrink: 0;
    background: var(--ax-border);
    cursor: row-resize;
  }

  .no-project {
    margin: 0;
    padding: var(--ax-space-4);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }

  /* The sidebar's right edge, dragged for its width. */
  .edge {
    width: var(--ax-space-1);
    flex-shrink: 0;
    background: var(--ax-border);
    cursor: col-resize;
  }

  .edge:hover,
  .edge.dragging {
    background: var(--ax-accent);
  }
</style>
