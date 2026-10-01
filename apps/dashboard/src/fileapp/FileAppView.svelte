<!--
  The file app's full-screen view (`docs/plans/editor.md`, ED1.3, F7, ED4.3):
  the title, opening files with the native dialog (⌘O) or from the recent
  list, the inspector's two buttons (settings, shortcuts; editor-look LK1) — and
  the tabs (W7, W12), each a `FileTab` with its own `FileEditor`, the same
  editor the panel and the IDE's file pane use. The inspector (`Inspector.svelte`)
  is a column of its own right of the tabs, so the minimap stays in view (K3).

  * **Hidden, never unmounted** — the same rule as the IDE: `App.svelte` keeps
    it mounted once opened, so unsaved text and the cursor survive a trip back
    to the OS. `inert` keeps the hidden view out of focus and tab order. The
    tabs behind the front one stay mounted the same way.
  * **Tabs in groups** (`fileDock.ts`, editor-look LK2): side by side or stacked
    (⌘\\, or a tab dragged to an edge), one tab per file in the whole layout,
    editors moved between groups without being rebuilt (`ide/paneStore.ts`).
  * **Tabs** (`tabs.ts`): one per file, a reusable preview tab, kept across
    restarts; ⌘W closes (asking first over unsaved text), ⌘N a new note,
    ⌃Tab/⌃⇧Tab and ⌘1–⌘9 switch. Editing in the preview tab fixes it.
  * **A panel hands its file over** (`handoff.ts`, W11): the tab takes the
    panel's live session, unsaved text and undo included.
  * **The tree** (`FileTree`, W6, W13) on the left — the workspace, projects,
    picked folders; ⌘B shows or hides it, its width is dragged; a rename there
    moves open tabs along (`FileSession.moved`).
  * **A file that cannot be opened is forgotten** from the recent list when it
    is gone or its root is; any other failure only says so. Its tab closes.
-->
<script lang="ts">
  import { onMount, tick, untrack } from "svelte";

  import { listenBackend, type FileRootInfo } from "../core/backend";
  import {
    changeProjectFolder,
    listProjects,
    listRoots,
    newProjectFolder,
    openProjectFolder,
    pickFile,
    removeProject,
    touchProject,
    type FileRenamed,
  } from "./backend";
  import type { OpenFileState, OpenResult } from "./FileEditor.svelte";
  import FileTab from "./FileTab.svelte";
  import Inspector, { type InspectorTab } from "./Inspector.svelte";
  import { inspectorSurface } from "./inspectorSurface.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import FileTree from "./FileTree.svelte";
  import ProjectSearch from "./ProjectSearch.svelte";
  import type { LocationList } from "./locationList";
  import QuickOpen from "./QuickOpen.svelte";
  import { handoffs, takeHandoffs, type Handoff } from "./handoff";
  import { forgetRecent, recentFiles, rememberRecent, type RecentFile } from "./recent";
  import { FOREIGN_ROOT } from "./session";
  import {
    activeTab,
    closeTab,
    cycleTab,
    emptyDock,
    focusedGroup,
    focusTab,
    loadDock,
    moveTabTo,
    nthTab,
    openInDock,
    pinTab,
    retargetTab,
    saveDock,
    splitActive,
    tabsOf,
    type FileDock,
    type FileRef,
    type Tab,
  } from "./fileDock";
  import { setFileDock } from "./fileDockContext";
  import DockNode from "../ide/DockNode.svelte";
  import FileGroup from "./FileGroup.svelte";
  import { DockDrag } from "../ide/dockDrag.svelte";
  import { allGroups, resizeSplit } from "../ide/layout";
  import { focusedIn, PANE_ATTR, parkPanes, placePanes, restoreFocus } from "../ide/paneStore";
  import { foldKey, forgetFolds } from "./foldMemory";
  import { uiScale } from "../core/uiScale";
  import { clampOutlineHeight, clampWidth, folderKey, loadTreePrefs, renamedPath, saveTreePrefs, type TreePrefs } from "./treeModel";
  import ProjectBar from "./ProjectBar.svelte";
  import OutlinePanel from "./OutlinePanel.svelte";
  import GitPanel from "./GitPanel.svelte";
  import GitDiffView from "./GitDiffView.svelte";
  import type { Side as GitSide } from "./gitBackend";
  import { pathAt } from "../editor/syntax/outline";
  import type { OutlineInfo } from "./outlineModel";
  import { projectRootId, resolveProject, treeRootsOf } from "./projectModel";
  import type { IdeProject } from "../core/backend";
  import UnsavedQuestion from "./UnsavedQuestion.svelte";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  /** The tabs, in groups side by side or stacked (LK2): the IDE's dock tree, with file rules (`fileDock.ts`). */
  let dock = $state<FileDock>(emptyDock());
  /** What each tab's editor reports — for titles, the unsaved dots, the header. */
  let states = $state<Record<string, OpenFileState | null>>({});
  let views = $state<Record<string, FileTab | null>>({});
  /** A panel's session waiting for the tab it was handed to (read once, when the tab loads). */
  const handedTo = new Map<string, Handoff["handed"]>();
  /** The line a new tab opens its file at (quick open's `:12`). */
  const lineFor = new Map<string, number>();
  let quickOpen = $state(false);
  /** The left column's tab (T13): the file tree, or the project search (⇧⌘F). */
  let sideTab = $state<"files" | "search" | "git">("files");
  /** The git panel (#48): the change open over the editor, how many files are changed, and a nudge to re-read. */
  let gitChange = $state<{ path: string; old_path: string | null; side: GitSide } | null>(null);
  let gitCount = $state(0);
  let gitNudge = $state(0);
  let gitVersion = $state(0);
  let gitSignature = "";
  let searchView = $state<ProjectSearch | null>(null);
  /** The tab being closed while it asks about unsaved text. */
  let closing = $state<{ id: string; answer: (close: boolean) => void } | null>(null);
  /** Tabs are saved only once the saved ones were read back, never over them (tracked: the save waits for it). */
  let restored = $state(false);

  let error = $state("");
  let recent = $state<RecentFile[]>([]);
  let roots = $state<FileRootInfo[]>([]);
  let showRecent = $state(false);
  /** The right-hand column (LK1, K2): which tab it shows, or closed. */
  let inspector = $state<InspectorTab | null>(null);
  /** What the settings' live preview draws with. */
  const preview = inspectorSurface();

  /** The header's two inspector buttons: open on that tab, or close it when it shows already. */
  function toggleInspector(tab: InspectorTab): void {
    inspector = inspector === tab ? null : tab;
  }
  let tree = $state<TreePrefs>(loadTreePrefs());
  let treeView = $state<FileTree | null>(null);
  /** The tree's width while its edge is being dragged. */
  let dragging = $state<{ startX: number; startWidth: number } | null>(null);
  /** What each tab's editor last reported for the outline (#49), by tab id. */
  let outlines = $state<Record<string, OutlineInfo | undefined>>({});
  /** The outline's top edge, dragged for its height. */
  let outlineDrag = $state<{ startY: number; startHeight: number } | null>(null);

  function startOutlineDrag(e: PointerEvent): void {
    outlineDrag = { startY: e.clientY, startHeight: tree.outlineHeight };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onOutlineDrag(e: PointerEvent): void {
    // Unscaled like the tree's width; the pointer moves in screen pixels, and up makes it taller.
    if (outlineDrag) {
      tree = { ...tree, outlineHeight: clampOutlineHeight(outlineDrag.startHeight - (e.clientY - outlineDrag.startY) / $uiScale) };
    }
  }

  /** The registry the editor shares with the IDE. */
  let projects = $state<IdeProject[]>([]);
  /** A project is being opened or made; the bar takes no second click meanwhile. */
  let projectBusy = $state(false);
  const currentProject = $derived(resolveProject(projects, tree.project));
  /** The tree shows the open project's folder and nothing else — a file opened alone is in no project. */
  const treeRoots = $derived(treeRootsOf(roots, currentProject?.id ?? null));
  const newId = () => crypto.randomUUID();
  /** Every tab, flat — the editors are rendered once each, in this order, and moved into their groups. */
  const allFileTabs = $derived(tabsOf(dock));
  /** The focused group's visible tab: the one the header and the keys are about. */
  const active = $derived(activeTab(dock));
  const activeOutline = $derived(active ? (outlines[active.id] ?? null) : null);

  /** The symbols around the cursor, outermost first. */
  const crumbs = $derived(activeOutline?.symbols ? pathAt(activeOutline.symbols, activeOutline.line) : []);

  function jumpToSymbol(line: number): void {
    if (active) views[active.id]?.goToLine(line);
  }
  /** The tabs on screen: every group's visible one. */
  const visibleTabs = $derived(new Set(allGroups(dock.layout).map((g) => g.active)));
  const current = $derived(active ? (states[active.id] ?? null) : null);

  function rootLabel(id: string): string {
    return roots.find((r) => r.id === id)?.label ?? id;
  }

  function tabTitle(tab: Tab): string {
    if (tab.file === null || states[tab.id]?.untitled) return "New note";
    return tab.file.rel.split("/").pop() ?? tab.file.rel;
  }

  /** Opens `file` (`null`: a new note) in a tab — the preview tab when only looking (W7). */
  function openTab(
    file: FileRef | null,
    preview = false,
    handed: Handoff["handed"] = null,
    line: number | null = null,
  ): void {
    showRecent = false;
    error = "";
    const result = openInDock(dock, file, { preview }, newId);
    if (handed && result.load) handedTo.set(result.target, handed);
    else if (handed) void keepAside(handed);
    if (line !== null && result.load) lineFor.set(result.target, line);
    else if (line !== null) views[result.target]?.goToLine(line);
    dock = result.dock;
    // A language server's read-only file outside every root (L11) is not one to come back to.
    if (file && !file.root.startsWith(FOREIGN_ROOT)) {
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

  async function reloadProjects(): Promise<void> {
    try {
      [projects, roots] = await Promise.all([listProjects(), listRoots()]);
    } catch (err) {
      error = `Could not read the projects: ${(err as { message?: string }).message ?? String(err)}`;
    }
  }

  /** Makes `project` the open one: the tree shows its folder, opened. Tabs already open stay as they are. */
  async function showProject(project: IdeProject): Promise<void> {
    await reloadProjects();
    const key = folderKey(projectRootId(project.id), "");
    tree = {
      ...tree,
      project: project.id,
      expanded: tree.expanded.includes(key) ? tree.expanded : [...tree.expanded, key],
    };
    sideTab = "files";
  }

  async function runProject(work: () => Promise<IdeProject | null>): Promise<void> {
    projectBusy = true;
    try {
      const project = await work();
      if (project) await showProject(project);
    } catch (err) {
      error = (err as { message?: string }).message ?? String(err);
    } finally {
      projectBusy = false;
    }
  }

  const pickProject = (id: number) => runProject(() => touchProject(id));
  const openProjectFolderDialog = () => runProject(openProjectFolder);
  const newProject = (name: string, gitInit: boolean) => runProject(() => newProjectFolder(name, gitInit));

  // Another project, or none: its change is not this one's.
  $effect(() => {
    void currentProject?.id;
    untrack(() => (gitChange = null));
  });

  /** "Change folder…": the project keeps its id; the tree follows if it is the open one. */
  async function changeFolder(id: number): Promise<void> {
    projectBusy = true;
    try {
      if (await changeProjectFolder(id)) await reloadProjects();
    } catch (err) {
      error = (err as { message?: string }).message ?? String(err);
    } finally {
      projectBusy = false;
    }
  }

  /** Removes the registry row (the IDE shares it); never the folder. The open project closes with it. */
  async function removeFromList(id: number): Promise<void> {
    try {
      await removeProject(id);
      if (tree.project === id) closeProject();
      await reloadProjects();
    } catch (err) {
      error = (err as { message?: string }).message ?? String(err);
    }
  }

  /** Takes the project out of the tree. Nothing on disk changes, open tabs stay, the IDE keeps the project. */
  function closeProject(): void {
    tree = { ...tree, project: null };
  }

  function onTabState(tab: Tab, state: OpenFileState | null): void {
    states[tab.id] = state;
    // Editing in the preview tab makes it a tab of its own (W7).
    if (state?.dirty && allFileTabs.find((t) => t.id === tab.id)?.preview) dock = pinTab(dock, tab.id);
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
    lineFor.delete(id);
    const file = allFileTabs.find((t) => t.id === id)?.file;
    // Folds are kept for the files in open tabs only (T7).
    if (file) forgetFolds(foldKey(file.root, file.rel));
    dock = closeTab(dock, id);
    delete states[id];
    delete views[id];
  }

  /** ⌘W, ×, Vi's `:q`: closes tab `id`, asking first over unsaved text (W11). */
  async function requestCloseTab(id: string): Promise<void> {
    // One question at a time: a second × while one is asked would leave the first unanswered.
    if (closing) return;
    const view = views[id];
    if (view?.hasUnsaved()) {
      dock = focusTab(dock, id);
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

  /** A rename (W13): the recent list follows. Open tabs follow on their own — their editors hear it too. */
  function onRenamed(renamed: FileRenamed): void {
    for (const file of recentFiles()) {
      const rel = file.root === renamed.root ? renamedPath(file.rel, renamed.from, renamed.to) : null;
      if (rel === null) continue;
      forgetRecent(file);
      rememberRecent({ root: file.root, rel });
    }
    recent = recentFiles();
  }

  function startDrag(e: PointerEvent): void {
    dragging = { startX: e.clientX, startWidth: tree.width };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onDrag(e: PointerEvent): void {
    // The width is unscaled, drawn times the UI scale (editor-look K10); the pointer moves in screen pixels.
    if (dragging) tree = { ...tree, width: clampWidth(dragging.startWidth + (e.clientX - dragging.startX) / $uiScale) };
  }

  /**
   * The view's own keys (W12), in the capture phase so they work with the
   * editor focused or not — and stopped there, like the editor's own ⌘S.
   */
  function onViewKeydown(e: KeyboardEvent): void {
    const key = e.key.toLowerCase();
    if (e.ctrlKey && !e.metaKey && !e.altKey && e.key === "Tab") {
      take(e);
      dock = cycleTab(dock, e.shiftKey ? -1 : 1);
      return;
    }
    // ⌘\ splits the visible tab off to the right, ⇧⌘\ below (LK2) — by the key's place, since
    // a German layout types "\" with ⌥⇧7.
    if (e.metaKey && !e.ctrlKey && (e.code === "Backslash" || e.key === "\\")) {
      take(e);
      dock = splitActive(dock, e.code === "Backslash" && e.shiftKey ? "bottom" : "right");
      return;
    }
    if (!e.metaKey || e.altKey || e.ctrlKey) return;
    if (!e.shiftKey && /^[1-9]$/.test(key)) {
      take(e);
      dock = nthTab(dock, Number(key));
    } else if (e.shiftKey && (key === "f" || e.code === "KeyF")) {
      take(e);
      void showSearch();
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
      if (active) void requestCloseTab(active.id);
    } else if (key === "b") {
      take(e);
      tree = { ...tree, visible: !tree.visible };
    } else if (key === "p") {
      take(e);
      quickOpen = !quickOpen;
    }
  }

  /** ⇧⌘F: the left column shows the project search, its field focused (T13). */
  async function showSearch(): Promise<void> {
    tree = { ...tree, visible: true };
    sideTab = "search";
    await tick();
    await searchView?.focus();
  }

  /** A language server's list (ED6.3): the left column's search shows it. */
  async function showLocations(list: LocationList): Promise<void> {
    tree = { ...tree, visible: true };
    sideTab = "search";
    await tick();
    searchView?.showLocations(list);
  }

  function take(e: KeyboardEvent): void {
    e.preventDefault();
    e.stopPropagation();
  }

  // ---- groups: dragging a tab, dragging a divider (LK2), as in the IDE (`ide/IdeView.svelte`) ----

  let dockEl = $state<HTMLElement | undefined>();
  let storeEl = $state<HTMLElement | undefined>();
  const drag = new DockDrag({
    dockEl: () => dockEl,
    layout: () => dock.layout,
    onPress: (tabId) => {
      dock = focusTab(dock, tabId);
    },
    onMove: (tabId, target) => {
      dock = moveTabTo(dock, tabId, target);
    },
    onResize: (splitId, boundary, fraction) => {
      dock = { ...dock, layout: resizeSplit(dock.layout, splitId, boundary, fraction) };
    },
  });

  setFileDock({
    title: tabTitle,
    where: (tab) => (tab.file ? `${rootLabel(tab.file.root)} / ${tab.file.rel}` : "New note"),
    dirty: (tabId) => states[tabId]?.dirty ?? false,
    focusedGroup: () => focusedGroup(dock).id,
    focusTab: (tabId) => {
      dock = focusTab(dock, tabId);
    },
    focusGroup: (groupId) => {
      if (dock.focus !== groupId) dock = { ...dock, focus: groupId };
    },
    pin: (tabId) => {
      dock = pinTab(dock, tabId);
    },
    requestClose: (tabId) => void requestCloseTab(tabId),
    startTabDrag: (tabId, event) => void drag.startTab(tabId, event),
    startDividerDrag: (splitId, boundary, event) => drag.startDivider(splitId, boundary, event),
    draggingTab: () => drag.draggingTab,
    hint: () => drag.hint,
  });

  // Keeping an editor alive across a layout change (`ide/paneStore.ts`): park every
  // editor before Svelte rebuilds the tree, place each into its group's slot after.
  // Parks on a change of `dock` only: the elements are read untracked. Tracked, the first tab —
  // which mounts the dock and binds `dockEl`/`storeEl` a moment later — parked its editor again
  // right after it was placed, and nothing placed it a second time (an open tab showing nothing).
  $effect.pre(() => {
    void dock;
    // Moving an editor takes the keyboard focus away from it: noted here, given back after the move.
    const focused = untrack(() => (dockEl ? focusedIn(dockEl) : null));
    untrack(() => {
      if (dockEl && storeEl) parkPanes(dockEl, storeEl);
    });
    // Place again once Svelte has rebuilt the tree. The effect below cannot promise to run after this
    // one: a change of `dock` made from inside an effect (the first edit pins the preview tab) reaches
    // it first, and the editor parked here would then stay in the store.
    void tick().then(() =>
      untrack(() => {
        if (dockEl) placePanes(dockEl);
        restoreFocus(focused);
      }),
    );
  });

  $effect(() => {
    void dock;
    if (dockEl) placePanes(dockEl);
  });

  /** Files handed over from a panel (W11), in the order they came. */
  function takeWaiting(): void {
    for (const h of takeHandoffs()) openTab(h.file, false, h.handed);
  }

  onMount(() => {
    dock = loadDock(newId);
    restored = true;
    takeWaiting();
    const unlistenRenamed = listenBackend<FileRenamed>("files:renamed", onRenamed);
    const unsubscribe = handoffs.subscribe((list) => {
      if (list.length > 0) takeWaiting();
    });
    window.addEventListener("blur", drag.abandon);
    return () => {
      window.removeEventListener("blur", drag.abandon);
      drag.abandon();
      unsubscribe();
      void unlistenRenamed.then((off) => off());
    };
  });

  $effect(() => {
    const snapshot = dock;
    if (!restored) return;
    saveDock(snapshot);
  });

  $effect(() => {
    const snapshot = tree;
    // Not while dragging: once, when the edge is let go.
    if (!dragging) saveTreePrefs(snapshot);
  });

  $effect(() => {
    if (open) {
      recent = recentFiles();
      void reloadProjects();
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
        <!-- Breadcrumbs (#49): the symbols holding the cursor, from the same data as the outline. -->
        {#each crumbs as crumb (crumb.line + "\0" + crumb.name)}
          <button type="button" class="crumb" onclick={() => jumpToSymbol(crumb.nameLine)}>
            <span aria-hidden="true">›</span> {crumb.name}
          </button>
        {/each}
      {/if}
    </div>
    <div class="actions">
      <IconButton icon="folder-open" label="Open a file… (⌘O)" onclick={() => void openPicked()} />
      <div class="recent-anchor">
        <IconButton
          icon="history"
          label="Recent files"
          pressed={showRecent}
          disabled={recent.length === 0}
          onclick={() => (showRecent = !showRecent)}
        />
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
      <IconButton
        icon="sliders-horizontal"
        label="Editor settings"
        pressed={inspector === "settings"}
        onclick={() => toggleInspector("settings")}
      />
      <IconButton
        icon="keyboard"
        label="Keyboard shortcuts"
        pressed={inspector === "shortcuts"}
        onclick={() => toggleInspector("shortcuts")}
      />
      <span class="separator" aria-hidden="true"></span>
      <IconButton icon="layout-grid" label="Back to the OS" onclick={() => (open = false)} />
    </div>
  </header>

  <div class="main">
  {#if tree.visible}
    <aside class="side" style:width="{tree.width * $uiScale}px">
      <ProjectBar
        {projects}
        current={currentProject}
        busy={projectBusy}
        onPick={(id) => void pickProject(id)}
        onOpenFolder={() => void openProjectFolderDialog()}
        onNew={(name, git) => void newProject(name, git)}
        onClose={closeProject}
        onChangeFolder={(id) => void changeFolder(id)}
        onRemove={(id) => void removeFromList(id)}
      />
      <div class="side-bar">
        <div class="side-tabs" role="tablist" aria-label="Left column">
          <IconButton icon="files" label="Files" tab pressed={sideTab === "files"} onclick={() => (sideTab = "files")} />
          <IconButton
            icon="text-search"
            tab
            label="Search the project (⇧⌘F)"
            pressed={sideTab === "search"}
            onclick={() => void showSearch()}
          />
          <IconButton
            icon="git-branch"
            tab
            label={gitCount > 0 ? `Git — ${gitCount} changed` : "Git"}
            pressed={sideTab === "git"}
            onclick={() => (sideTab = "git")}
          />
        </div>
        {#if sideTab === "files"}
          <IconButton
            icon={tree.showHidden ? "eye" : "eye-off"}
            label={tree.showHidden ? "Hide dotfiles, .git, node_modules, target" : "Show dotfiles, .git, node_modules, target"}
            pressed={tree.showHidden}
            size="sm"
            onclick={() => (tree = { ...tree, showHidden: !tree.showHidden })}
          />
          <IconButton icon="refresh-cw" label="Read the folders again" size="sm" onclick={() => treeView?.refresh()} />
        {/if}
      </div>
      <!-- Both stay mounted: the tree keeps what is open, the search its results. -->
      <div class="side-pane" class:gone={sideTab !== "search"}>
        <ProjectSearch
          bind:this={searchView}
          roots={treeRoots}
          initialRoot={active?.file?.root ?? null}
          onOpen={(root, rel, line) => openTab({ root, rel }, true, null, line)}
        />
      </div>
      <div class="side-pane" class:gone={sideTab !== "git"}>
        <GitPanel
          root={currentProject ? projectRootId(currentProject.id) : null}
          active={open && sideTab === "git"}
          selected={gitChange ? { path: gitChange.path, side: gitChange.side } : null}
          refresh={gitNudge}
          onOpen={(row) => (gitChange = { path: row.entry.path, old_path: row.entry.old_path, side: row.side })}
          onStatus={(count, status) => {
            gitCount = count;
            // The open change follows the status, but only when the list of changes really changed.
            const signature = JSON.stringify(status?.entries ?? []);
            if (signature !== gitSignature) {
              gitSignature = signature;
              gitVersion++;
            }
          }}
        />
      </div>
      <div class="side-pane" class:gone={sideTab !== "files"}>
      <div class="tree-area">
      <FileTree
        bind:this={treeView}
        roots={treeRoots}
        bind:expanded={tree.expanded}
        showHidden={tree.showHidden}
        active={active?.file ?? null}
        onOpen={(file, preview) => openTab(file, preview)}
        onError={(message) => (error = message)}
      />
      {#if treeRoots.length === 0}
        <p class="no-project">
          {currentProject ? "The project folder is not available." : "No project open. Open a folder or start a new project from the bar above."}
        </p>
      {/if}
      </div>
      {#if tree.outlineOpen}
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
      <div class="outline-area" style:height={tree.outlineOpen ? `${tree.outlineHeight * $uiScale}px` : "auto"}>
        <OutlinePanel
          info={activeOutline}
          open={tree.outlineOpen}
          onToggle={() => (tree = { ...tree, outlineOpen: !tree.outlineOpen })}
          onJump={jumpToSymbol}
        />
      </div>
      </div>
    </aside>
    <div
      class="edge"
      class:dragging={dragging !== null}
      role="separator"
      aria-orientation="vertical"
      aria-label="Tree width"
      onpointerdown={startDrag}
      onpointermove={onDrag}
      onpointerup={() => (dragging = null)}
    ></div>
  {/if}
  <div class="column">
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

  {#if allFileTabs.length > 0}
    <div class="dock" class:dragging={drag.draggingTab !== null} bind:this={dockEl}>
      <!-- Every editor is rendered here once and only ever *moved* into its group's
           slot (`ide/paneStore.ts`), so moving a tab to another group keeps its
           cursor, undo history and unsaved text. Hidden with `visibility`, filling
           the area, so an editor waiting here still measures its real size. -->
      <div class="pane-store" bind:this={storeEl} aria-hidden="true">
        {#each allFileTabs as tab (tab.id)}
          <div class="pane" {...{ [PANE_ATTR]: tab.id }}>
            <FileTab
              bind:this={views[tab.id]}
              file={tab.file}
              handed={handedTo.get(tab.id) ?? null}
              line={lineFor.get(tab.id) ?? null}
              visible={open && visibleTabs.has(tab.id)}
              onOpenRequest={() => void openPicked()}
              onQuit={() => void requestCloseTab(tab.id)}
              onState={(state) => onTabState(tab, state)}
              onMoved={(file) => (dock = retargetTab(dock, tab.id, file))}
              onFailed={(result) => onTabFailed(tab, result)}
              onOpenFile={(file, line) => openTab(file, false, null, line)}
              onShowLocations={(list) => void showLocations(list)}
              onOutline={(info) => (outlines[tab.id] = info)}
            />
          </div>
        {/each}
      </div>
      <DockNode node={dock.layout.root} onDividerDown={(splitId, boundary, event) => drag.startDivider(splitId, boundary, event)}>
        {#snippet group(g)}<FileGroup group={g} />{/snippet}
      </DockNode>
      {#if drag.rootHint}
        <div class="root-highlight {drag.rootHint}"></div>
      {/if}
    </div>
  {:else}
  <div class="stack">
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
  </div>
  {/if}
  {#if gitChange && currentProject}
    <GitDiffView
      root={projectRootId(currentProject.id)}
      entry={gitChange}
      side={gitChange.side}
      version={gitVersion}
      onClose={() => (gitChange = null)}
      onChanged={() => gitNudge++}
      onOpenFile={(line) => {
        if (!gitChange || !currentProject) return;
        const file = { root: projectRootId(currentProject.id), rel: gitChange.path };
        gitChange = null;
        openTab(file, true, null, line);
      }}
    />
  {/if}
  </div>
  {#if inspector}
    <Inspector tab={inspector} surface={preview.current} onTab={(t) => (inspector = t)} onClose={() => (inspector = null)} />
  {/if}
  </div>
  {#if quickOpen}
    <QuickOpen
      roots={treeRoots}
      {recent}
      onOpen={(file, preview, line) => openTab(file, preview, null, line)}
      onClose={() => (quickOpen = false)}
    />
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

  .crumb {
    flex-shrink: 0;
    padding: 0 var(--ax-space-1);
    background: none;
    border: 0;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    white-space: nowrap;
    cursor: pointer;
  }

  .crumb:last-of-type {
    color: var(--ax-accent);
  }

  .crumb:hover {
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

  /* Between the view's own buttons and the way back to the OS. */
  .separator {
    width: 1px;
    height: var(--ax-icon-md);
    margin: 0 var(--ax-space-1);
    background: var(--ax-border);
  }

  kbd {
    margin-left: var(--ax-space-1);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
  }

  .no-project {
    margin: 0;
    padding: var(--ax-space-4);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }

  .recent-anchor {
    position: relative;
  }

  .recent {
    position: absolute;
    right: 0;
    top: calc(100% + var(--ax-space-1));
    z-index: 10;
    min-width: calc(320px * var(--ax-ui-scale));
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

  /* The tree beside the tabs. */
  .main {
    flex: 1;
    min-height: 0;
    display: flex;
  }

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

  .side-tabs {
    flex: 1;
    display: flex;
    gap: var(--ax-space-1);
  }

  .side-pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
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

  .side-pane.gone {
    display: none;
  }


  /* The tree's right edge, dragged for its width. */
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

  /* Tab bar, banners and the tabs, beside the tree. */
  .column {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  /* The tabs lie on top of each other here; only the front one is visible (FileTab). */
  .stack {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
  }

  /* The groups (LK2): the tree fills the column; editors wait in the store until placed. */
  .dock {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
  }

  .dock.dragging {
    cursor: grabbing;
  }

  /* While a tab is dragged, the editors must not take the pointer (and its hover). */
  .dock.dragging :global([data-ide-slot]) {
    pointer-events: none;
  }

  .pane-store {
    position: absolute;
    inset: 0;
    visibility: hidden;
    pointer-events: none;
  }

  /* Absolute in the store and in a group's slot alike: both are positioned, so it fills either. */
  .pane {
    position: absolute;
    inset: 0;
    display: flex;
  }

  .root-highlight {
    position: absolute;
    z-index: 10;
    pointer-events: none;
    background: var(--ax-accent-muted);
    border: 2px solid var(--ax-accent);
    border-radius: var(--ax-radius-sm);
  }

  .root-highlight.left {
    top: 0;
    bottom: 0;
    left: 0;
    width: 25%;
  }

  .root-highlight.right {
    top: 0;
    bottom: 0;
    right: 0;
    width: 25%;
  }

  .root-highlight.top {
    left: 0;
    right: 0;
    top: 0;
    height: 25%;
  }

  .root-highlight.bottom {
    left: 0;
    right: 0;
    bottom: 0;
    height: 25%;
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
    min-width: calc(360px * var(--ax-ui-scale));
  }
</style>
