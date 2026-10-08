<!--
  The full-screen IDE (milestone M7.1, CP2): a dock layout of panes, each one
  hosting a module that already exists. The first pane content is the Terminal.

  The view owns the layout and nothing else owns any of it: `ide/layout.ts`
  decides what a dock, a move or a close does to the tree, `ide/dock.ts` turns
  pointer positions into dock targets, and this file does the two things only a
  component can — mount the panes, and measure the screen.

  Three decisions worth knowing before changing anything here:

  * **The view is hidden, never unmounted.** `App.svelte` keeps it mounted once
    it has been opened and this component hides itself with `visibility` and
    `inert`. Unmounting would destroy every pane, and a terminal pane closes
    its PTY session on destroy — going back to the dashboard for a moment would
    kill whatever was running in every shell. `inert` is what keeps a hidden
    pane out of the tab order and away from the keyboard.
  * **Escape does not close the IDE.** It belongs to whatever is inside the
    pane — a terminal running vim needs it far more than the view needs a
    shortcut. The way out is the button, deliberately.
  * **Drag geometry is measured once, when the drag starts.** The snapshot is
    exactly the approach `core/kanban.ts` takes for card drags; re-measuring on
    every pointer move would read a layout the drag highlight is already
    changing.

  CP3 adds projects: this layout is per-project and persists into the
  `layout_json` the CP0 crate already stores. Until then it starts fresh with
  one terminal and lives only as long as the app does.
-->
<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { fade } from "svelte/transition";

  import { on } from "../core/bus";
  import { toast } from "../core/toast";

  import { DockDrag } from "./dockDrag.svelte";
  import { setDock } from "./dockContext";
  import DockNode from "./DockNode.svelte";
  import PaneGroup from "./PaneGroup.svelte";
  import { uiScale } from "../core/uiScale";
  import Icon from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import Inspector, { type InspectorTab } from "../fileapp/Inspector.svelte";
  import { inspectorSurface } from "../fileapp/inspectorSurface.svelte";
  import {
    activateTab,
    addTab,
    allGroups,
    allTabs,
    closeTab,
    findTab,
    moveTab,
    resizeSplit,
    setTabConfig,
    type Layout,
    type PaneTab,
  } from "./layout";
  import type { AgentSpec, IdeAgent } from "../core/backend";
  import AgentsPanel from "./AgentsPanel.svelte";
  import AgentsSettings from "./AgentsSettings.svelte";
  import TasksPanel from "./TasksPanel.svelte";
  import DebugPanel from "./DebugPanel.svelte";
  import { onDebugReveal, onDebugTerminal } from "./debug";
  import { followRename, loadBreakpoints } from "./breakpoints";
  import { forgetTaskRun, startTaskRun } from "./taskRuns";
  import { taskCommandLine, type TaskInfo } from "./tasksBackend";
  import ActivityRail from "./ActivityRail.svelte";
  import { agentStatus } from "./agentStatus";
  import { foldKey, forgetFolds } from "../fileapp/foldMemory";
  import QuickOpen from "../fileapp/QuickOpen.svelte";
  import type { FileRef } from "../fileapp/tabs";
  import {
    FILES_PANE,
    fileTab,
    filePaneConfig,
    fileOrigin,
    frontFile,
    frontFileTab,
    FILE_PANE,
    isWorkPane,
    TASK_PANE,
    taskIdOf,
    taskTab,
    layoutAfterRename,
    openFilePreview,
    pinFileTab,
    retargetFileTab,
    untitledTab,
    openOrFocus,
    projectRoot,
    showsFile,
  } from "./paneKinds";
  import { applyProjectCwd } from "./paneCwd";
  import { MODES, MODE_ICON, MODE_LABEL, parkedLayouts, type Mode } from "./modes";
  import { balanceCanvas, canvasAgentTarget } from "./canvasLayout";
  import { agentTabsOf, balanceFlow, flowAgentTarget, modeForAgent, resetFlowPanes } from "./planning";
  import { get } from "svelte/store";
  import { agentRequests, takeAgentRequests } from "./agentRequest";
  import { modeRequest } from "./modeRequest";
  import { projectRequest } from "./projectRequest";
  import { fileHandle, stashHandover } from "./fileHandles";
  import { takeHandoffs, handoffs } from "../fileapp/handoff";
  import { recentFiles, rememberRecent } from "../fileapp/recent";
  import { FOREIGN_ROOT } from "../fileapp/session";
  import UnsavedQuestion from "../fileapp/UnsavedQuestion.svelte";
  import { cycleTab, nthTab, splitActive } from "./dockKeys";
  import PaneHost from "./panes/PaneHost.svelte";
  import { focusedIn, PANE_ATTR, parkPanes, placePanes, restoreFocus } from "./paneStore";
    import * as projectSession from "./projectSession";
  import { flushLayout } from "./projects";
  import ProjectSidebar from "../fileapp/ProjectSidebar.svelte";
  import GitDiffView from "../fileapp/GitDiffView.svelte";
  import type { Side as GitSide } from "../fileapp/gitBackend";
  import type { OutlineInfo } from "../fileapp/outlineModel";
  import { pathAt } from "../editor/syntax/outline";
  import { listRoots, pickFile, type FileRenamed } from "../fileapp/backend";
  import { invokeBackend, listenBackend } from "../core/backend";
  import { treeRootsOf } from "../fileapp/projectModel";
  import { loadTreePrefs, saveTreePrefs, type SidebarView, type TreePrefs } from "../fileapp/treeModel";
  import type { FileRootInfo } from "../core/backend";

  /** The task id of the pane the debugger uses when it runs a program in a terminal. */
  const DEBUG_TERMINAL = "debug:session";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  // Which project is open, and the list, live in `projectSession` — this
  // component owns the dock tree and the two drag engines, and nothing else.
  const sessionState = projectSession.session;
  const projects = $derived($sessionState.projects);
  const current = $derived($sessionState.current);
  const agents = $derived($sessionState.agents);
  const mode = $derived($sessionState.mode);
  /** The other modes' layouts: their panes stay mounted (hidden), so an agent keeps running while the files are shown. */
  const parked = $derived($sessionState.parked);

  let layout = $state<Layout>(projectSession.noProjectLayout());
  let dockEl = $state<HTMLElement | undefined>();
  const drag = new DockDrag({
    dockEl: () => dockEl,
    layout: () => layout,
    onPress: (tabId) => {
      layout = activateTab(layout, tabId);
    },
    onMove: (tabId, target) => {
      layout = moveTab(layout, tabId, target);
    },
    onResize: (splitId, boundary, fraction) => {
      layout = resizeSplit(layout, splitId, boundary, fraction);
    },
  });
  /** The right-hand inspector (editor-look I5): the editor's settings and the shortcuts, as in the file app. */
  let inspector = $state<InspectorTab | null>(null);
  const preview = inspectorSurface();
  /** The shared sidebar (`fileapp/ProjectSidebar.svelte`) and its own prefs, saved apart from the editor's. */
  let sidebar = $state<ProjectSidebar | null>(null);
  let tree = $state<TreePrefs>(loadTreePrefs("ide"));
  let roots = $state<FileRootInfo[]>([]);
  const treeRoots = $derived(treeRootsOf(roots, current?.id ?? null));
  /** The symbols of each open file, by `root\0rel`; the front file's go to the sidebar's outline. */
  let outlines = $state<Record<string, OutlineInfo>>({});
  const front = $derived(frontFile(layout));
  const activeOutline = $derived(front ? (outlines[`${front.root}\0${front.rel}`] ?? null) : null);
  /** The symbols holding the front file's cursor — the breadcrumbs (#49), from the same data as the outline. */
  const crumbs = $derived(activeOutline?.symbols ? pathAt(activeOutline.symbols, activeOutline.line) : []);
  let gitCount = $state(0);
  const statuses = agentStatus.statuses;
  /** The agents with a pane open in either layout (a hidden mode's pane still runs). */
  const openAgentIds = $derived(
    new Set(
      [...allTabs(layout), ...parkedLayouts(parked).flatMap(allTabs)].flatMap((t) =>
        t.kind === "agent" && typeof t.config?.agentId === "number" ? [t.config.agentId] : [],
      ),
    ),
  );
  const agentsRunning = $derived([...$statuses.byAgent.values()].filter((v) => v?.state === "working").length);

  /** The rail: a click on another view shows it; a click on the one shown folds the column away. */
  function selectView(view: SidebarView): void {
    tree = tree.visible && tree.view === view ? { ...tree, visible: false } : { ...tree, view, visible: true };
  }

  /** A change open in the diff view over the dock. */
  let gitChange = $state<{ path: string; old_path: string | null; side: GitSide } | null>(null);
  let gitNudge = $state(0);
  let gitVersion = $state(0);
  let gitSignature = "";
  /** ⌘P over the open project's files (W9). */
  let quickOpen = $state(false);

  /** The open project as quick open's only root. */
  const projectRoots = $derived(
    current
      ? [{ id: projectRoot(current.id), label: current.name, path: current.repo_root, kind: "project" as const }]
      : [],
  );
  /** The files open in the dock, for quick open to rank first. */
  const openFiles = $derived(
    allTabs(layout).flatMap((t): FileRef[] => {
      const c = filePaneConfig(t);
      return c && !c.untitled ? [{ root: c.root, rel: c.rel }] : [];
    }),
  );

  /**
   * Opens a file from quick open in the dock's file group, at `line` if given —
   * or, with no file open yet, beside the Files pane, as a click in it would.
   */
  function openFromQuickOpen(file: FileRef, line: number | null, preview = false): void {
    const beside = allTabs(layout).find((t) => t.kind === FILES_PANE)?.id ?? null;
    const from = fileOrigin(layout, beside, lastWorkTab);
    layout = preview
      ? openFilePreview(layout, file, line, from)
      : openOrFocus(layout, fileTab(file.root, file.rel, line), (t) => showsFile(t, file.root, file.rel), from);
    // Not a language server's read-only file (`lsp:`): it is not one to come back to.
    if (!file.root.startsWith(FOREIGN_ROOT)) rememberRecent(file);
  }

  /** ⌘N: a new note in the file group — one draft at a time, as in the editor. */
  function newNote(): void {
    const from = fileOrigin(layout, null, lastWorkTab);
    layout = openOrFocus(layout, untitledTab(), (t) => filePaneConfig(t)?.untitled === true, from);
  }

  /**
   * Files handed over from the floating panel (W11), in the order they came: the Editor mode shows them,
   * each with the panel's live session (unsaved text, undo history). One whose file already has a tab
   * keeps its text aside instead.
   */
  async function takeWaiting(): Promise<void> {
    for (const h of takeHandoffs()) {
      if (mode !== "editor" && current) switchTo("editor");
      const shown = h.file && allTabs(layout).find((t) => showsFile(t, h.file!.root, h.file!.rel));
      if (shown) {
        if (h.handed) {
          await h.handed.session.persistRecovery();
          await h.handed.session.close();
        }
        layout = activateTab(layout, shown.id);
        continue;
      }
      const tab = h.file ? fileTab(h.file.root, h.file.rel, null) : untitledTab();
      if (h.handed) stashHandover(tab.id, h.handed);
      const from = fileOrigin(layout, null, lastWorkTab);
      layout = openOrFocus(layout, tab, (t) => t.id === tab.id, from);
      if (h.file && !h.file.root.startsWith(FOREIGN_ROOT)) rememberRecent(h.file);
    }
  }

  /**
   * The pane the user last worked in — a terminal, an agent, a file; never
   * the Files tree or the Search pane. A first file opened from those helpers
   * becomes a tab in its group (`fileOrigin`). Followed by focus and by
   * pointer, since a terminal's canvas takes clicks without moving focus —
   * and, because a pane can hold the focus without ever having been clicked
   * (a terminal focuses itself when it starts; a reload forgets this value),
   * also by asking which pane still has the focus the moment the user turns
   * to the tree or to ⌘P.
   */
  let lastWorkTab = $state<string | null>(null);

  /** The work pane `el` sits in (or whose tab it is), if any. */
  function workPaneOf(el: Element | null): string | null {
    const id =
      el?.closest(`[${PANE_ATTR}]`)?.getAttribute(PANE_ATTR) ??
      el?.closest("[data-ide-tab]")?.getAttribute("data-ide-tab") ??
      null;
    const tab = id ? allTabs(layout).find((t) => t.id === id) : undefined;
    return tab && isWorkPane(tab) ? tab.id : null;
  }

  /** Remembers the work pane that still has the focus, before it moves. */
  function noteFocusedPane(): void {
    lastWorkTab = workPaneOf(document.activeElement) ?? lastWorkTab;
  }

  function noteWorkPane(event: Event): void {
    // A pointer press comes before the focus moves: whatever held it is
    // where the user comes from, even when that pane was never clicked.
    if (event.type === "pointerdown") noteFocusedPane();
    lastWorkTab = workPaneOf(event.target as Element | null) ?? lastWorkTab;
  }

  /**
   * The view's own keys, in the capture phase so a focused terminal does not
   * get them first: ⌘P (quick open) and ⇧⌘F (the Search pane, T13).
   * Everything else belongs to the panes.
   */
  function onViewKeydown(e: KeyboardEvent): void {
    // The editor's tab keys, on the group the user last worked in.
    if (e.ctrlKey && !e.metaKey && !e.altKey && e.key === "Tab") {
      e.preventDefault();
      e.stopPropagation();
      noteFocusedPane();
      layout = cycleTab(layout, lastWorkTab, e.shiftKey ? -1 : 1);
      return;
    }
    if (e.metaKey && !e.ctrlKey && !e.altKey && (e.code === "Backslash" || e.key === "\\")) {
      e.preventDefault();
      e.stopPropagation();
      noteFocusedPane();
      layout = splitActive(layout, lastWorkTab, e.code === "Backslash" && e.shiftKey ? "bottom" : "right");
      return;
    }
    if (!e.metaKey || e.altKey || e.ctrlKey) return;
    const key = e.key.toLowerCase();
    if (!e.shiftKey && /^[1-9]$/.test(key)) {
      e.preventDefault();
      e.stopPropagation();
      noteFocusedPane();
      layout = nthTab(layout, lastWorkTab, Number(key));
      return;
    }
    if (!e.shiftKey && key === "w") {
      // ⌘W closes a *file* tab only: a terminal or agent holds a live shell.
      noteFocusedPane();
      const tab = allTabs(layout).find((t) => t.id === lastWorkTab);
      if (tab && filePaneConfig(tab)) {
        e.preventDefault();
        e.stopPropagation();
        void requestClose(tab.id);
      }
      return;
    }
    if (!e.shiftKey && key === "n") {
      e.preventDefault();
      e.stopPropagation();
      newNote();
      return;
    }
    if (!e.shiftKey && key === "o") {
      e.preventDefault();
      e.stopPropagation();
      void openPicked();
      return;
    }
    if (!e.shiftKey && key === "b") {
      e.preventDefault();
      e.stopPropagation();
      tree.visible = !tree.visible;
      return;
    }
    if (!current) return;
    if (!e.shiftKey && key === "p") {
      e.preventDefault();
      e.stopPropagation();
      if (!quickOpen) noteFocusedPane();
      quickOpen = !quickOpen;
    } else if (e.shiftKey && (key === "f" || e.code === "KeyF")) {
      e.preventDefault();
      e.stopPropagation();
      showSearch();
    }
  }

  /** ⇧⌘F: the sidebar's Search tab with its field focused — the sidebar is shown first if it was folded away. */
  async function showSearch(): Promise<void> {
    tree.visible = true;
    await tick();
    await sidebar?.showSearch();
  }

  /** Opening a project replaces the tree; its old panes are unmounted, which
   *  is the one place in this view where destroying a pane is right — those
   *  terminals were running in a different project's folder. */
  async function openProjectById(id: number) {
    const next = await projectSession.open(id);
    if (next) layout = next;
  }

  async function openFolderAsProject() {
    const next = await projectSession.openFolder();
    if (next) layout = next;
  }

  async function addProject(name: string, gitInit: boolean) {
    const next = await projectSession.createFolder(name, gitInit);
    if (next) layout = next;
  }

  async function changeRoot(id: number) {
    const next = await projectSession.changeRoot(id, layout);
    if (next) layout = next;
  }

  async function removeProject(id: number) {
    if (await projectSession.remove(id)) layout = projectSession.noProjectLayout();
  }

  async function closeProject() {
    await projectSession.close();
    layout = projectSession.noProjectLayout();
    gitChange = null;
  }

  /** The sidebar's Files tab opens a file as a tab of the dock's file group. */
  function openFromSidebar(file: FileRef, preview: boolean, line: number | null = null): void {
    openFromQuickOpen(file, line, preview);
  }

  /** The file tab being asked about before it closes, and how the user answered. */
  let closing = $state<{ id: string; name: string; answer: (close: boolean) => void } | null>(null);

  /** Closes `tabId`, asking first over unsaved text in a file pane; one question at a time. */
  async function requestClose(tabId: string): Promise<void> {
    if (closing) return;
    const handle = fileHandle(tabId);
    if (handle?.hasUnsaved()) {
      layout = activateTab(layout, tabId);
      const tab = allTabs(layout).find((t) => t.id === tabId);
      const name = (tab && filePaneConfig(tab)?.rel) || "This file";
      const close = await new Promise<boolean>((resolve) => {
        closing = {
          id: tabId,
          name,
          answer: (answer) => {
            closing = null;
            resolve(answer);
          },
        };
      });
      if (!close) return;
    }
    dockClose(tabId);
  }

  async function saveAndClose(): Promise<void> {
    const c = closing;
    if (c && (await fileHandle(c.id)?.saveNow())) c.answer(true);
  }

  async function discardAndClose(): Promise<void> {
    const c = closing;
    if (!c) return;
    await fileHandle(c.id)?.discard();
    c.answer(true);
  }

  /**
   * Runs a task: its shell line is resolved in Rust (a project task not yet confirmed is refused there),
   * then typed into a terminal pane — a second click on the same task starts that pane again. A new pane
   * docks below the dock, or joins the group of the task pane already open.
   */
  async function runTask(task: Pick<TaskInfo, "id" | "label">): Promise<void> {
    const project = current;
    if (!project) return;
    let line: string;
    try {
      line = await taskCommandLine(projectRoot(project.id), task.id);
    } catch (err) {
      toast((err as { message?: string }).message ?? String(err), "danger");
      return;
    }
    const existing = allTabs(layout).find((t) => taskIdOf(t) === task.id);
    if (existing) {
      startTaskRun(existing.id, line);
      layout = activateTab(layout, existing.id);
      return;
    }
    const tab = taskTab(task.label, task.id);
    startTaskRun(tab.id, line);
    const other = allTabs(layout).find((t) => t.kind === TASK_PANE);
    const group = other ? findTab(layout, other.id)?.group.id : undefined;
    layout = group
      ? addTab(layout, tab, { nodeId: group, side: "center" })
      : addTab(layout, tab, { nodeId: layout.root.id, side: "bottom" });
  }

  /**
   * The debugger asked for the program to run in a terminal (a TUI, a program that reads the keyboard): it is
   * typed into a task-style pane called “Debug”, reused by the next session. The line is never stored.
   */
  function openDebugTerminal(title: string, line: string): void {
    const existing = allTabs(layout).find((t) => taskIdOf(t) === DEBUG_TERMINAL);
    if (existing) {
      startTaskRun(existing.id, line);
      layout = activateTab(layout, existing.id);
      return;
    }
    const tab = taskTab(title || "Debug", DEBUG_TERMINAL);
    startTaskRun(tab.id, line);
    const other = allTabs(layout).find((t) => t.kind === TASK_PANE);
    const group = other ? findTab(layout, other.id)?.group.id : undefined;
    layout = group
      ? addTab(layout, tab, { nodeId: group, side: "center" })
      : addTab(layout, tab, { nodeId: layout.root.id, side: "bottom" });
  }

  function restartTask(tabId: string): void {
    const tab = allTabs(layout).find((t) => t.id === tabId);
    const id = tab ? taskIdOf(tab) : null;
    if (id === DEBUG_TERMINAL) {
      toast("Start the debug session again from the Debug view.", "info");
      return;
    }
    if (tab && id) void runTask({ id, label: tab.title });
  }

  /** ⌘O: the native file dialog; a picked file opens as a tab of the file group. */
  async function openPicked(): Promise<void> {
    try {
      const picked = await pickFile();
      if (picked && !picked.folder) openFromQuickOpen({ root: picked.root, rel: picked.rel }, null);
    } catch (err) {
      toast(`Could not open the dialog: ${(err as { message?: string }).message ?? String(err)}`, "danger");
    }
  }

  /** The outline's click: the front file pane moves its cursor (its `jump` changes, as for a diff). */
  function jumpToSymbol(line: number): void {
    const tab = frontFileTab(layout);
    const config = tab ? filePaneConfig(tab) : null;
    if (!tab || !config) return;
    layout = setTabConfig(layout, tab.id, { ...config, line, jump: Date.now() });
  }

  /** Switches the shown mode; the layout on screen is parked, not closed. */
  function switchTo(next: Mode): void {
    if (!current || next === mode) return;
    layout = projectSession.switchMode(layout, next);
  }

  /**
   * Puts an agent into a pane, beside whatever is already open.
   *
   * Docked to the right of the group holding the active pane rather than as
   * another tab in it: agents are watched, not switched between — the whole
   * point of the dock is seeing more than one at a time. A tab only carries
   * the agent's *id*; the profile itself stays in one place, so editing it
   * does not mean hunting down copies in a stored layout.
   */
  function openAgent(agent: IdeAgent, preferred?: Mode, background = false) {
    // Already open: bring that pane forward instead of starting a second copy of the agent — a second pane would start
    // a second harness on the same session. It may sit in a mode that is not shown; then that mode is shown.
    const here = agentTabsOf(layout, [agent.id])[0];
    if (here) {
      // A session the studio started by itself is not brought forward: it has its pane, the owner keeps their own view.
      if (!background) layout = activateTab(layout, here.id);
      return;
    }
    const shown = get(projectSession.session);
    const elsewhere = MODES.find(
      (other) => other !== shown.mode && shown.parked[other] && agentTabsOf(shown.parked[other], [agent.id]).length > 0,
    );
    if (elsewhere) {
      if (background) return;
      layout = projectSession.switchMode(layout, elsewhere);
      const found = agentTabsOf(layout, [agent.id])[0];
      if (found) layout = activateTab(layout, found.id);
      return;
    }
    const tab: PaneTab = {
      id: crypto.randomUUID(),
      kind: "agent",
      title: agent.name,
      config: { agentId: agent.id },
    };
    // A session the studio started by itself is put where it belongs and the owner stays where they are.
    if (background && preferred && preferred !== shown.mode) {
      projectSession.addTabParked(preferred, tab);
      // The layout of the mode shown is saved with every change, the hidden ones only through it: written now.
      projectSession.save(layout);
      return;
    }
    if (preferred && preferred !== shown.mode) layout = projectSession.switchMode(layout, preferred);
    // In the Flow the pane goes beside the team's tiles and the session columns share their room evenly
    // (`balanceFlow`); on the Canvas the agents share the width (`canvasLayout.ts`).
    const flowTarget = flowAgentTarget(layout);
    const added = addTab(layout, tab, flowTarget ?? canvasAgentTarget(layout));
    // A fourth Flow pane becomes a tab in the newest column: no new column, so the owner's sizes stay.
    const flowBalanced = flowTarget?.side === "center" ? added : balanceFlow(added);
    layout = flowTarget ? flowBalanced : balanceCanvas(added);
  }

  /**
   * Opens a terminal beside what is already there.
   *
   * The tab bar's own `+` adds one *into* a group; this one splits, because
   * the header is where you reach for a second thing to look at rather than a
   * second tab in the pane you are in. Same target as opening an agent.
   */
  function openTerminal() {
    const project = current;
    if (!project) return;
    const groups = allGroups(layout);
    const tab = projectSession.terminalTab();
    // On a Canvas with agents the terminal takes the left quarter, as it does when an agent opens beside one.
    const beside = mode === "agents" && allTabs(layout).some((t) => t.kind === "agent");
    const last = groups.length > 0 ? groups[groups.length - 1].id : layout.root.id;
    const added = beside
      ? balanceCanvas(addTab(layout, tab, { nodeId: layout.root.id, side: "left" }))
      : addTab(layout, tab, { nodeId: last, side: "right" });
    layout = applyProjectCwd(added, project.repo_root);
  }

  async function addAgent(spec: AgentSpec) {
    const created = await projectSession.addAgent(spec);
    if (created) openAgent(created);
  }

  /** The plan a card belongs to, or `null` (no card, no plan, or a board that cannot be read): found on the boards. */
  async function planOfCard(cardId: number | null): Promise<number | null> {
    if (cardId === null) return null;
    try {
      const boards = await invokeBackend<{ id: number }[]>("list_boards");
      for (const board of boards) {
        const cards = await invokeBackend<{ id: number; plan_id: number | null }[]>("list_board_cards", {
          boardId: board.id,
          includeArchived: false,
        });
        const found = cards.find((card) => card.id === cardId);
        if (found) return found.plan_id;
      }
    } catch {
      // Without the lookup the pane opens on the Canvas, as it did before.
    }
    return null;
  }

  /**
   * A card was started and the session made for it should be on screen: its project open, the Agents mode shown, its
   * pane docked. The agent list is read again first — the session did not exist when it was last read.
   */
  async function showRequestedAgents(): Promise<void> {
    for (const { projectId, agentId, background } of takeAgentRequests()) {
      try {
        if (get(projectSession.session).current?.id !== projectId) {
          // A session the studio started by itself in another project does not take the owner out of theirs: it is said,
          // and its pane opens when the project does (`openCardPanesOf`, run whenever a project opens).
          if (background === true) {
            toast(
              `A card is running in another project (session #${agentId}); its pane appears as soon as you open it.`,
              "info",
            );
            continue;
          }
          const next = await projectSession.open(projectId);
          if (next) layout = next;
        }
        await projectSession.refreshAgents();
        const agent = projectSession.agentById(agentId);
        if (!agent) continue;
        // A planner belongs to the plan it plans: its pane opens in the Flow, beside the planning panel.
        openAgent(agent, modeForAgent(agent, await planOfCard(agent.card_id ?? null)), background === true);
      } catch (err) {
        const why = err instanceof Error ? err.message : String(err);
        toast(`The session could not be opened: ${why}`, "danger");
      }
    }
  }

  /**
   * The panes of the card sessions that run in `projectId` and have none yet. A card the studio started while another
   * project was open has a session whose harness starts only when its pane does; the request that named it was answered
   * with a note, so it is made good here, when its project opens.
   */
  async function openCardPanesOf(projectId: number): Promise<void> {
    try {
      const open = await invokeBackend<{ card_id: number; project_id: number; agent_id: number }[]>("open_card_sessions");
      const mine = open.filter((session) => session.project_id === projectId);
      if (mine.length === 0) return;
      await projectSession.refreshAgents();
      for (const session of mine) {
        const agent = projectSession.agentById(session.agent_id);
        if (agent) openAgent(agent, modeForAgent(agent), true);
      }
    } catch {
      // Without the list the sessions can still be opened from the Agents list.
    }
  }

  let lastCatchUp: number | null = null;
  $effect(() => {
    const id = current?.id ?? null;
    if (id === null || id === lastCatchUp) return;
    lastCatchUp = id;
    untrack(() => void openCardPanesOf(id));
  });

  /** The last arrangement must not be left in a timer when the app goes away. */
  function flushOnLeaving() {
    void flushLayout();
  }

  onMount(() => {
    // The most recently opened project comes first out of the store, so
    // reopening the IDE lands where the user left off without a stored
    // "current project" of its own to drift out of step.
    // Hand-overs wait until the project's own layout is in place — it would replace them otherwise.
    let unsubscribeHandoffs = () => {};
    let unsubscribeMode = () => {};
    let unsubscribeProject = () => {};
    let unsubscribeAgent = () => {};
    // A plan that has had its say ends its planner; the pane that showed it has nothing left to run.
    const unsubscribeClose = on("studio:close-agent-panes", (detail) => {
      const ids = (detail as { agentIds?: number[] } | undefined)?.agentIds ?? [];
      layout = agentTabsOf(layout, ids).reduce((acc, tab) => closeTab(acc, tab.id), layout);
      projectSession.closeParkedAgentTabs(ids);
    });
    // "Anordnung zurücksetzen" in the Flow's bar: the three panels go back where they started.
    const unsubscribeReset = on("studio:reset-flow", () => {
      if (get(sessionState).mode !== "flow") return;
      projectSession.save((layout = resetFlowPanes(layout)));
    });
    let gone = false;
    void projectSession.start().then((next) => {
      if (next) layout = next;
      if (gone) return;
      unsubscribeHandoffs = handoffs.subscribe((list) => {
        if (list.length > 0) void takeWaiting();
      });
      unsubscribeMode = modeRequest.subscribe((wanted) => {
        if (!wanted) return;
        modeRequest.set(null);
        switchTo(wanted);
      });
      unsubscribeProject = projectRequest.subscribe((wanted) => {
        if (wanted === null) return;
        projectRequest.set(null);
        void openProjectById(wanted);
      });
      unsubscribeAgent = agentRequests.subscribe((waiting) => {
        if (waiting.length > 0) void showRequestedAgents();
      });
    });
    void listRoots().then((r) => (roots = r)).catch(() => {});
    // A rename in the tree reaches the open file tabs: their stored path (and so the layout kept per project) follows.
    const unlistenRenamed = listenBackend<FileRenamed>("files:renamed", (r) => {
      layout = layoutAfterRename(layout, r.root, r.from, r.to);
      followRename(r.root, r.from, r.to);
    });

    loadBreakpoints();
    onDebugTerminal(openDebugTerminal);
    onDebugReveal((root, rel, line) => openFromQuickOpen({ root, rel }, Math.max(0, line - 1)));
    window.addEventListener("blur", drag.abandon);
    // `pagehide` is what `core/persist.ts` uses for the same job: a quit while
    // the IDE is still the view on screen would otherwise drop the last write.
    window.addEventListener("pagehide", flushOnLeaving);
    return () => {
      gone = true;
      onDebugReveal(null);
      onDebugTerminal(null);
      unsubscribeHandoffs();
      unsubscribeMode();
      unsubscribeProject();
      unsubscribeAgent();
      unsubscribeClose();
      unsubscribeReset();
      void unlistenRenamed.then((off) => off());
      window.removeEventListener("blur", drag.abandon);
      window.removeEventListener("pagehide", flushOnLeaving);
      // Today the view is never unmounted, but its correctness must not depend
      // on a caller-side invariant it cannot enforce — HMR alone breaks it.
      drag.abandon();
    };
  });

  /** Closes a tab at once, whatever is in it. */
  function dockClose(tabId: string): void {
    // A file pane's folds are kept only while it is open (T7).
    const file = allTabs(layout).find((t) => t.id === tabId);
    const config = file ? filePaneConfig(file) : null;
    if (config) forgetFolds(foldKey(config.root, config.rel));
    if (file?.kind === TASK_PANE) forgetTaskRun(tabId);
    layout = closeTab(layout, tabId);
  }

  setDock({
    activate: (tabId) => {
      layout = activateTab(layout, tabId);
    },
    showLocations: (list) => {
      tree.visible = true;
      void tick().then(() => sidebar?.showLocations(list));
    },
    close: dockClose,
    requestClose: (tabId) => void requestClose(tabId),
    restartTask,
    startTabDrag: (tabId, event) => {
      if (drag.startTab(tabId, event)) event.preventDefault();
    },
    startDividerDrag: (splitId, boundary, event) => drag.startDivider(splitId, boundary, event),
    open: (tab, match, fromTabId) => {
      const from = tab.kind === FILE_PANE ? fileOrigin(layout, fromTabId, lastWorkTab) : fromTabId;
      layout = openOrFocus(layout, tab, match, from);
    },
    pin: (tabId) => {
      layout = pinFileTab(layout, tabId);
    },
    filed: (tabId, file) => {
      layout = retargetFileTab(layout, tabId, file, null, false);
    },
    reportOutline: (file, info) => {
      outlines[`${file.root}\0${file.rel}`] = info;
    },
    setConfig: (tabId, config) => {
      layout = setTabConfig(layout, tabId, config);
    },
    activeFile: () => frontFile(layout),
    draggingTab: () => drag.draggingTab,
    hint: () => drag.hint,
  });

  /** Every pane in the layout, flat — the store renders exactly this list. */
  const panes = $derived([...allTabs(layout), ...parkedLayouts(parked).flatMap(allTabs)]);
  /** The panes on screen: the active tab of every group, while the view is open. */
  const visibleTabs = $derived(new Set(open ? allGroups(layout).map((g) => g.active) : []));

  let storeEl = $state<HTMLElement | undefined>();

  // The two halves of keeping a pane alive across a layout change. Park before
  // Svelte touches the DOM, because a slot being destroyed would take the pane
  // inside it along; place after, into whatever slots now exist. Reading
  // `layout` in both is what ties them to the change. See `ide/paneStore.ts`.
  $effect.pre(() => {
    void layout;
    // Untracked elements: see `fileapp/FileAppView.svelte` — parking again when the elements get bound
    // would undo the placement, and nothing places a second time.
    const focused = untrack(() => (dockEl ? focusedIn(dockEl) : null));
    untrack(() => {
      if (dockEl && storeEl) parkPanes(dockEl, storeEl);
    });
    // Place once the tree is rebuilt, and give the focus back (a moved pane loses it); see
    // `fileapp/FileAppView.svelte` for why the effect below is not enough.
    void tick().then(() =>
      untrack(() => {
        if (dockEl) placePanes(dockEl);
        restoreFocus(focused);
      }),
    );
  });

  $effect(() => {
    void layout;
    if (dockEl) placePanes(dockEl);
  });

  // A project opened or added is a new root for the file service: read the list again.
  $effect(() => {
    void $sessionState.projects;
    void listRoots().then((r) => (roots = r)).catch(() => {});
  });

  // The sidebar's prefs are written with a delay, and not while its edge is being dragged.
  $effect(() => {
    const snapshot = $state.snapshot(tree);
    if (!sidebar?.dragging()) saveTreePrefs(snapshot, "ide");
  });

  // Every change to the tree queues a write of the open project's layout. The
  // write that follows opening a project stores what was just read back, which
  // costs one statement and buys not having to track a dirty flag that could
  // be wrong in the other direction — a layout silently not saved.
  $effect(() => {
    projectSession.save(layout);
  });

  // Leaving the IDE is the last moment anyone is watching, and the app may be
  // closed from the dashboard next. Don't leave the last arrangement in a timer.
  $effect(() => {
    if (!open) void flushLayout();
  });

  // Agent statuses are polled only while somebody can see them: the IDE is on
  // screen and a project is open (CP6). Closing the view stops the tick.
  $effect(() => {
    agentStatus.watch(open ? (current?.id ?? null) : null);
  });
  onMount(() => () => agentStatus.watch(null));
</script>

<section class="ide" class:hidden={!open} inert={!open} aria-label="Studio" onkeydowncapture={onViewKeydown}>
  <header>
    <div class="titles">
      <div class="brand" style:--tree-w="{tree.visible ? tree.width * $uiScale : 0}px">
        <h1>Studio</h1>
        <div class="modes" role="group" aria-label="Mode">
          {#each MODES as each (each)}
            <button
              type="button"
              class:on={mode === each}
              disabled={!current}
              title={MODE_LABEL[each]}
              aria-label={MODE_LABEL[each]}
              onclick={() => switchTo(each)}
            >
              <Icon name={MODE_ICON[each]} size="md" />{#if mode === each}{MODE_LABEL[each]}{/if}
            </button>
          {/each}
        </div>
      </div>
      {#if front && crumbs.length > 0}
        {#each crumbs as crumb (crumb.line + "\0" + crumb.name)}
          <button type="button" class="crumb" onclick={() => jumpToSymbol(crumb.nameLine)}>
            <span aria-hidden="true">›</span> {crumb.name}
          </button>
        {/each}
      {/if}
    </div>
    <div class="actions">
      <IconButton
        size="lg"
        icon="settings"
        label="Editor settings"
        pressed={inspector === "settings"}
        onclick={() => (inspector = inspector === "settings" ? null : "settings")}
      />
      <IconButton
        size="lg"
        icon="keyboard"
        label="Keyboard shortcuts"
        pressed={inspector === "shortcuts"}
        onclick={() => (inspector = inspector === "shortcuts" ? null : "shortcuts")}
      />
      <span class="separator" aria-hidden="true"></span>
      <IconButton size="lg" icon="layout-grid" label="Back to the OS" onclick={() => (open = false)} />
    </div>
  </header>

  {#if closing}
    <UnsavedQuestion
      name={closing.name}
      untitled={false}
      onSave={() => void saveAndClose()}
      onDiscard={() => void discardAndClose()}
      onCancel={() => closing?.answer(false)}
    />
  {/if}
  <div class="body">
  <ActivityRail
    view={tree.view}
    open={tree.visible}
    {gitCount}
    {agentsRunning}
    disabled={!current}
    onSelect={selectView}
    onTerminal={openTerminal}
    onOpenFile={() => void openPicked()}
  />
  <!-- Folded away, not unmounted: the tree keeps what is open, the search its results, the git panel its status. -->
  <div class="side-wrap" class:gone={!tree.visible}>
    <ProjectSidebar
      bind:this={sidebar}
      bind:prefs={tree}
      {open}
      {projects}
      {current}
      busy={$sessionState.switching}
      onPickProject={(id) => void openProjectById(id)}
      onOpenFolder={() => void openFolderAsProject()}
      onNewProject={(name, git) => void addProject(name, git)}
      onCloseProject={() => void closeProject()}
      onChangeFolder={(id) => void changeRoot(id)}
      onRemoveProject={(id) => void removeProject(id)}
      roots={treeRoots}
      active={front}
      onOpenFile={openFromSidebar}
      onOpenResult={(root, rel, line) => openFromSidebar({ root, rel }, true, line)}
      onError={(message) => toast(message, "danger")}
      outline={activeOutline}
      onJumpToSymbol={jumpToSymbol}
      gitRoot={current ? projectRoot(current.id) : null}
      gitSelected={gitChange ? { path: gitChange.path, side: gitChange.side } : null}
      gitRefresh={gitNudge}
      onOpenChange={(row) => (gitChange = { path: row.entry.path, old_path: row.entry.old_path, side: row.side })}
      onGitStatus={(count, status) => {
        const signature = JSON.stringify(status?.entries ?? []);
        if (signature !== gitSignature) {
          gitSignature = signature;
          gitVersion++;
        }
        gitCount = count;
      }}
    >
      {#snippet agentsView()}
        <AgentsPanel
          {agents}
          {openAgentIds}
          disabled={!current}
          projectId={current?.id ?? null}
          onManage={() => (inspector = "agents")}
          onOpen={openAgent}
          onCreate={(spec) => void addAgent(spec)}
          onEdit={(id, spec) => void projectSession.editAgent(id, spec)}
          onRemove={(id) => void projectSession.removeAgent(id)}
        />
      {/snippet}
      {#snippet tasksView()}
        <TasksPanel
          root={current ? projectRoot(current.id) : null}
          active={tree.visible && tree.view === "tasks"}
          onRun={(task) => void runTask(task)}
          onError={(message) => toast(message, "danger")}
        />
      {/snippet}
      {#snippet debugView()}
        <DebugPanel
          root={current ? projectRoot(current.id) : null}
          folder={current?.repo_root ?? null}
          active={tree.visible && tree.view === "debug"}
          currentFile={frontFile(layout)}
          onOpen={(rel, line) => current && openFromQuickOpen({ root: projectRoot(current.id), rel }, line - 1)}
          onError={(message) => toast(message, "danger")}
        />
      {/snippet}
    </ProjectSidebar>
  </div>
  <div class="dock-column">
  <div
    class="dock"
    class:dragging={drag.draggingTab !== null}
    bind:this={dockEl}
    onfocusin={noteWorkPane}
    onpointerdowncapture={noteWorkPane}
  >
    <!-- Where panes live. They are rendered here once and only ever *moved*
         into the tree's slots, so that dragging a pane does not destroy and
         rebuild it — which closed an agent's PTY and restarted it, for every
         pane on screen, on every structural change. The store fills the dock
         and is hidden with `visibility` rather than `display: none`, so a pane
         waiting here still measures its real size. -->
    <div class="pane-store" bind:this={storeEl} aria-hidden="true">
      {#each panes as tab (tab.id)}
        <div class="pane-slot" {...{ [PANE_ATTR]: tab.id }}>
          <PaneHost
            {tab}
            visible={visibleTabs.has(tab.id)}
            onConfig={(config) => projectSession.save((layout = setTabConfig(layout, tab.id, config)))}
          />
        </div>
      {/each}
    </div>

    {#if current || panes.length > 0}
      <DockNode node={layout.root} onDividerDown={(splitId, boundary, event) => drag.startDivider(splitId, boundary, event)}>
        {#snippet group(g)}<PaneGroup group={g} />{/snippet}
      </DockNode>
      {#if mode === "agents" && current && allTabs(layout).length === 0}
        <!-- A new project's Canvas is empty on purpose (no terminal nobody asked for): say so, and offer the terminal. -->
        <div class="canvas-empty">
          <p>The canvas is empty. Agents appear here as soon as a card or a session starts.</p>
          <button class="ax-btn primary" type="button" onclick={openTerminal}>Open terminal</button>
        </div>
      {/if}
      {#if drag.rootHint && drag.rootHint !== "center"}
        <div class="root-highlight {drag.rootHint}" transition:fade={{ duration: 80 }}></div>
      {/if}
    {:else}
      <!-- No project, no panes: a terminal with nowhere to start is worse than
           no terminal. The menu above is the only thing to do here. -->
      <div class="empty">
        <p>Open a project — from the project bar in the sidebar.</p>
      </div>
    {/if}
  </div>
  {#if gitChange && current}
    <GitDiffView
      root={projectRoot(current.id)}
      entry={gitChange}
      side={gitChange.side}
      version={gitVersion}
      onClose={() => (gitChange = null)}
      onChanged={() => gitNudge++}
      onOpenFile={(line) => {
        if (!gitChange || !current) return;
        const file = { root: projectRoot(current.id), rel: gitChange.path };
        gitChange = null;
        openFromQuickOpen(file, line);
      }}
    />
  {/if}
  </div>
  {#if inspector}
    <Inspector tab={inspector} surface={preview.current} onTab={(t) => (inspector = t)} onClose={() => (inspector = null)}>
      {#snippet agents()}<AgentsSettings />{/snippet}
    </Inspector>
  {/if}
  </div>
  {#if quickOpen && open}
    <QuickOpen
      roots={projectRoots}
      recent={[...openFiles, ...recentFiles()]}
      onOpen={(file, _preview, line) => openFromQuickOpen(file, line)}
      onClose={() => (quickOpen = false)}
    />
  {/if}
</section>

<style>
  .ide {
    /* The rail's width, shared with the header block that is centred over the rail and the tree. */
    --ide-rail-w: calc(57px * var(--ax-ui-scale));
    position: fixed;
    inset: 0;
    z-index: calc(var(--ax-z-staging) - 1);
    display: flex;
    flex-direction: column;
    background: var(--ax-bg);
    background-image: var(--ax-texture-url);
    color: var(--ax-text);
  }

  /* Hidden, not unmounted — see the header comment. `visibility` keeps every
     pane's measured size, which a terminal needs to keep its PTY in step. */
  .ide.hidden {
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
    flex: 0 0 auto;
  }

  .titles {
    display: flex;
    align-items: center;
    gap: var(--ax-space-3);
    min-width: 0;
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

  /* The name stays at the left edge; the mode switch is centred in what is left over the rail and the tree, and follows
     the tree's width when it is dragged (owner, 2026-10-08). Only the mode shown carries its name, so the row fits. */
  .brand {
    display: flex;
    align-items: center;
    gap: var(--ax-space-4);
    min-width: calc(var(--ide-rail-w) + var(--tree-w, 0px));
    /* The block starts at the window's edge, over the rail: the header's own side padding is taken back. */
    margin-left: calc(-1 * var(--ax-space-5));
    padding: 0 var(--ax-space-4);
    box-sizing: border-box;
  }

  h1 {
    margin: 0;
    font-family: var(--ax-font-display);
    font-size: var(--ax-font-size-xl);
    letter-spacing: var(--ax-tracking-wide);
  }

  /* The mode switch is the Studio's main choice: the same filled-accent selection as the Second Brain bar's toggles,
     larger than the quiet icon buttons around it (owner, 2026-10-08: the buttons were easy to overlook). */
  .modes {
    display: flex;
    gap: var(--ax-space-1);
    padding: calc(3px * var(--ax-ui-scale));
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    background: var(--ax-surface-2);
    margin-inline: auto;
  }

  .modes button {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-3);
    border: 0;
    border-radius: var(--ax-radius-sm);
    background: transparent;
    color: var(--ax-text);
    font: inherit;
    font-size: var(--ax-font-size-base);
    font-weight: 600;
    cursor: pointer;
    transition:
      background var(--ax-dur-fast) var(--ax-ease),
      color var(--ax-dur-fast) var(--ax-ease);
  }

  .modes button:hover:not(:disabled):not(.on) {
    background: var(--ax-surface-3);
  }

  .modes button.on {
    background: var(--ax-accent);
    color: var(--ax-text-invert);
  }

  .modes button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .actions {
    /* The header's icons follow the rail's bigger ones (owner, 2026-10-08). */
    --ax-hit-min: calc(34px * var(--ax-ui-scale));
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
  }

  .separator {
    width: 1px;
    height: var(--ax-icon-lg);
    margin: 0 var(--ax-space-1);
    background: var(--ax-border);
  }

  /* The dock and, beside it, the inspector. */
  .body {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
  }

  /* Groups meet edge to edge, a line between them (editor-look I1). */
  .side-wrap {
    display: contents;
  }

  .side-wrap.gone {
    display: none;
  }

  .dock-column {
    position: relative;
    flex: 1 1 auto;
    min-width: 0;
    min-height: 0;
    display: flex;
  }

  .dock {
    position: relative;
    flex: 1 1 auto;
    min-width: 0;
    min-height: 0;
  }

  .pane-store {
    position: absolute;
    inset: 0;
    visibility: hidden;
    pointer-events: none;
  }

  .pane-slot {
    position: absolute;
    inset: 0;
  }

  /* Over the empty dock, but not in the way of it: a pane can still be dropped here, only the button takes the pointer. */
  .canvas-empty {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--ax-space-3);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
    pointer-events: none;
  }
  .canvas-empty p {
    margin: 0;
  }
  .canvas-empty button {
    pointer-events: auto;
  }

  .empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }

  /* While a tab is in flight the panes must not react to the pointer passing
     over them — a terminal would start selecting text under the drag. */
  .dock.dragging :global(.pane-host) {
    pointer-events: none;
  }

  .root-highlight {
    position: absolute;
    pointer-events: none;
    background: var(--ax-accent-muted);
    border: 2px solid var(--ax-accent);
    border-radius: var(--ax-radius-md);
  }

  .root-highlight.left {
    top: 0;
    bottom: 0;
    left: 0;
    width: 33%;
  }

  .root-highlight.right {
    top: 0;
    bottom: 0;
    right: 0;
    width: 33%;
  }

  .root-highlight.top {
    left: 0;
    right: 0;
    top: 0;
    height: 33%;
  }

  .root-highlight.bottom {
    left: 0;
    right: 0;
    bottom: 0;
    height: 33%;
  }
</style>
