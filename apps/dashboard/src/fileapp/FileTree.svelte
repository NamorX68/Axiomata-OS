<!--
  The file tree (`docs/plans/editor.md`, ED4, W6, W13): the roots the file app
  may show — the workspace, IDE projects, picked folders — with their folders
  loaded when opened (`file_list`), never all at once.

  * **A click opens a file in the preview tab, a double click for good** (W7);
    a click on a folder opens or closes it. The IDE's Files pane has no preview
    tab and opens a dock pane either way (`onOpen`'s `preview` is a hint).
  * **Hidden by default**: dotfiles, `.git`, `node_modules`, `target` (W6);
    what a `.gitignore` ignores is shown greyed out.
  * **Acting on entries** (right click or ⋯): New file, New folder, Rename,
    Delete — names typed in place; a folder's delete names how many entries
    go with it. Open copies of a renamed or deleted file learn of it from the
    backend (`files:renamed`, `files:removed`), not from the tree.
  * **No tree watcher** (W14): a folder is read again when opened, after an
    action in it, and with ↻.
  * **Dragging moves** (ED5, T11): a file or folder dropped on a folder of the
    same root moves there (`file_rename`, never replacing; open copies follow
    through `files:renamed`); a closed folder hovered for half a second opens.
    Pointer events, not HTML drag and drop, which the Tauri window may take for
    dropping files from Finder.
-->
<script lang="ts">
  import type { FileRootInfo } from "../core/backend";
  import { messageOf } from "../core/errors";
  import { countTree, createFile, deleteTree, listDir, makeDir, renameEntry, type Listing } from "./backend";
  import type { FileRef } from "./tabs";
  import Icon from "../ui/Icon.svelte";
  import FileIcon from "./FileIcon.svelte";
  import {
    baseName,
    expandedAfterDelete,
    expandedAfterRename,
    folderKey,
    isHidden,
    joinRel,
    moveTarget,
    nameProblem,
    parentOf,
  } from "./treeModel";

  interface Props {
    roots: FileRootInfo[];
    /** Open folders, as `folderKey`s — kept by the owner across restarts. */
    expanded: string[];
    showHidden: boolean;
    /** The file in the front tab, highlighted. */
    active: FileRef | null;
    onOpen: (file: FileRef, preview: boolean) => void;
    onError: (message: string) => void;
  }

  let { roots, expanded = $bindable(), showHidden, active, onOpen, onError }: Props = $props();

  type Loaded = Listing | "loading" | { error: string };
  let listings = $state<Record<string, Loaded>>({});

  /** A name being typed in place. */
  type Edit =
    | { kind: "new-file" | "new-dir"; root: string; dir: string; value: string }
    | { kind: "rename"; root: string; rel: string; value: string };
  let editing = $state<Edit | null>(null);
  let problem = $state<string | null>(null);
  /** Delete asked about: the entry and how many go with it. */
  let confirming = $state<{ root: string; rel: string; count: number | null } | null>(null);
  /** The ⋯ / right-click menu. */
  let menu = $state<{ root: string; rel: string; kind: "root" | "dir" | "file"; x: number; y: number } | null>(null);

  const isOpen = (root: string, rel: string) => expanded.includes(folderKey(root, rel));

  /** Pixels the pointer moves with the button down before it is a drag, not a click. */
  const DRAG_THRESHOLD_PX = 5;
  /** How long a closed folder is hovered during a drag before it opens (T11). */
  const OPEN_ON_HOVER_MS = 500;

  /** A drag in progress: what is dragged, where the pointer is, and the folder under it. */
  interface Drag {
    root: string;
    rel: string;
    name: string;
    x: number;
    y: number;
    over: { root: string; rel: string } | null;
  }
  let drag = $state<Drag | null>(null);
  /** The click that ends a drag must not open or toggle the row it ends on. */
  let swallowClick = false;
  let hoverTimer: ReturnType<typeof setTimeout> | undefined;
  /** The closed folder the open-on-hover timer runs for. */
  let hoverKey: string | null = null;

  /** What the drop would do, for the ghost and the highlight. */
  const dropping = $derived(drag?.over ? moveTarget(drag, drag.over) : null);

  /** The folder a drop at `x`/`y` goes into — a folder row, a file row's folder, a root — and that row. */
  function dropAt(x: number, y: number): { dir: { root: string; rel: string }; row: HTMLElement } | null {
    const row = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-drop-root]");
    const root = row?.dataset.dropRoot;
    if (!row || root === undefined) return null;
    return { dir: { root, rel: row.dataset.dropDir ?? "" }, row };
  }

  /** A closed folder under the pointer opens after a moment; another one restarts the wait. */
  function openOnHover(row: HTMLElement | null): void {
    const folder = row?.dataset.folder;
    const root = row?.dataset.dropRoot;
    const key = folder !== undefined && root !== undefined && !isOpen(root, folder) ? folderKey(root, folder) : null;
    if (key === hoverKey) return;
    clearTimeout(hoverTimer);
    hoverKey = key;
    if (key === null || root === undefined || folder === undefined) return;
    hoverTimer = setTimeout(() => {
      if (drag && !isOpen(root, folder)) toggle(root, folder);
    }, OPEN_ON_HOVER_MS);
  }

  /** A pointer went down on an entry: past a few pixels of movement it is a drag. */
  function onEntryPointerDown(e: PointerEvent, root: string, rel: string, name: string): void {
    if (e.button !== 0 || editing) return;
    const start = { x: e.clientX, y: e.clientY };
    let started = false;
    const onMove = (ev: PointerEvent) => {
      if (!started && Math.hypot(ev.clientX - start.x, ev.clientY - start.y) < DRAG_THRESHOLD_PX) return;
      started = true;
      menu = null;
      const at = dropAt(ev.clientX, ev.clientY);
      drag = { root, rel, name, x: ev.clientX, y: ev.clientY, over: at?.dir ?? null };
      openOnHover(at?.row ?? null);
    };
    const end = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("keydown", onKey, true);
      clearTimeout(hoverTimer);
      hoverKey = null;
    };
    const onUp = () => {
      end();
      if (!started) return;
      swallowClick = true;
      const done = drag;
      drag = null;
      if (done?.over) void moveInto(done, done.over);
    };
    const onKey = (ev: KeyboardEvent) => {
      if (ev.key !== "Escape") return;
      ev.preventDefault();
      ev.stopPropagation();
      end();
      if (started) swallowClick = true;
      drag = null;
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("keydown", onKey, true);
  }

  /** Moves the dragged entry into `dir` (T11); a refusal or a failure is said, not thrown. */
  async function moveInto(from: Drag, dir: { root: string; rel: string }): Promise<void> {
    const target = moveTarget(from, dir);
    if (target === null) return;
    if ("refused" in target) {
      onError(target.refused);
      return;
    }
    try {
      await renameEntry(from.root, from.rel, target.to);
      expanded = expandedAfterRename(expanded, from.root, from.rel, target.to);
      if (!isOpen(dir.root, dir.rel)) expanded = [...expanded, folderKey(dir.root, dir.rel)];
    } catch (err) {
      onError(messageOf(err));
    }
    await Promise.all([load(from.root, parentOf(from.rel)), load(dir.root, dir.rel)]);
  }

  /** Whether the folder `root`/`rel` is where the drag would drop. */
  const isDropTarget = (root: string, rel: string) =>
    drag?.over?.root === root && drag.over.rel === rel && dropping !== null && !("refused" in dropping);

  async function load(root: string, rel: string): Promise<void> {
    const key = folderKey(root, rel);
    if (!listings[key]) listings[key] = "loading";
    try {
      listings[key] = await listDir(root, rel);
    } catch (err) {
      listings[key] = { error: messageOf(err) };
    }
  }

  function toggle(root: string, rel: string): void {
    const key = folderKey(root, rel);
    if (expanded.includes(key)) {
      expanded = expanded.filter((k) => k !== key);
      return;
    }
    expanded = [...expanded, key];
    void load(root, rel);
  }

  /** ↻: every open folder read again. */
  export function refresh(): void {
    for (const key of expanded) {
      const [root, rel] = key.split("\0");
      void load(root, rel ?? "");
    }
  }

  // Open folders (restored ones too) are read when the tree first shows them.
  $effect(() => {
    for (const key of expanded) {
      if (!listings[key]) {
        const [root, rel] = key.split("\0");
        void load(root, rel ?? "");
      }
    }
  });

  function openMenu(e: MouseEvent, root: string, rel: string, kind: "root" | "dir" | "file"): void {
    e.preventDefault();
    e.stopPropagation();
    menu = { root, rel, kind, x: e.clientX, y: e.clientY };
  }

  function startNew(kind: "new-file" | "new-dir", root: string, dir: string): void {
    menu = null;
    problem = null;
    if (!isOpen(root, dir)) toggle(root, dir);
    editing = { kind, root, dir, value: "" };
  }

  function startRename(root: string, rel: string): void {
    menu = null;
    problem = null;
    editing = { kind: "rename", root, rel, value: baseName(rel) };
  }

  async function startDelete(root: string, rel: string, isDir: boolean): Promise<void> {
    menu = null;
    confirming = { root, rel, count: null };
    if (isDir) {
      try {
        const count = await countTree(root, rel);
        if (confirming?.rel === rel) confirming = { root, rel, count };
      } catch (err) {
        confirming = null;
        onError(messageOf(err));
      }
    }
  }

  async function commitEdit(): Promise<void> {
    const edit = editing;
    if (!edit) return;
    problem = nameProblem(edit.value);
    if (problem) return;
    const name = edit.value.trim();
    try {
      if (edit.kind === "rename") {
        const to = joinRel(parentOf(edit.rel), name);
        if (to !== edit.rel) {
          await renameEntry(edit.root, edit.rel, to);
          expanded = expandedAfterRename(expanded, edit.root, edit.rel, to);
        }
        editing = null;
        await load(edit.root, parentOf(edit.rel));
        return;
      }
      const rel = joinRel(edit.dir, name);
      if (edit.kind === "new-dir") await makeDir(edit.root, rel);
      else await createFile(edit.root, rel);
      editing = null;
      await load(edit.root, edit.dir);
      if (edit.kind === "new-file") onOpen({ root: edit.root, rel }, false);
    } catch (err) {
      problem = messageOf(err);
    }
  }

  async function commitDelete(): Promise<void> {
    const c = confirming;
    if (!c) return;
    confirming = null;
    try {
      await deleteTree(c.root, c.rel);
      expanded = expandedAfterDelete(expanded, c.root, c.rel);
    } catch (err) {
      onError(messageOf(err));
    }
    await load(c.root, parentOf(c.rel));
  }

  function onEditKey(e: KeyboardEvent): void {
    if (e.key === "Enter") {
      e.preventDefault();
      void commitEdit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      editing = null;
      problem = null;
    }
  }

  function autofocus(node: HTMLInputElement): void {
    node.focus();
    node.select();
  }

  const isActive = (root: string, rel: string) => active?.root === root && active.rel === rel;
</script>

<svelte:window onclick={() => (menu = null)} />

{#snippet nameInput(depth: number)}
  <div class="row edit" style:--depth={depth}>
    <input
      class="name-input"
      aria-label="Name"
      bind:value={editing!.value}
      onkeydown={onEditKey}
      onblur={() => {
        if (!problem) editing = null;
      }}
      use:autofocus
    />
  </div>
  {#if problem}<div class="row problem" style:--depth={depth}>{problem}</div>{/if}
{/snippet}

{#snippet folder(root: string, rel: string, depth: number)}
  {@const loaded = listings[folderKey(root, rel)]}
  {#if editing && editing.kind !== "rename" && editing.root === root && editing.dir === rel}
    {@render nameInput(depth)}
  {/if}
  {#if loaded === "loading" || loaded === undefined}
    <div class="row note" style:--depth={depth}>Loading…</div>
  {:else if "error" in loaded}
    <div class="row note error" style:--depth={depth}>{loaded.error}</div>
  {:else}
    {#each loaded.entries.filter((e) => showHidden || !isHidden(e.name)) as entry (entry.name)}
      {@const path = joinRel(rel, entry.name)}
      {#if editing?.kind === "rename" && editing.root === root && editing.rel === path}
        {@render nameInput(depth)}
      {:else}
        <div
          class="line"
          class:active={isActive(root, path)}
          class:drop-target={entry.kind === "dir" && isDropTarget(root, path)}
          class:dragged={drag?.root === root && drag.rel === path}
          data-drop-root={root}
          data-drop-dir={entry.kind === "dir" ? path : rel}
          data-folder={entry.kind === "dir" ? path : undefined}
        >
          <button
            type="button"
            class="row"
            class:ignored={entry.ignored}
            style:--depth={depth}
            title={entry.kind === "link" ? `${path} (a link)` : path}
            onpointerdown={(e) => onEntryPointerDown(e, root, path, entry.name)}
            onclick={() =>
              entry.kind === "dir" ? toggle(root, path) : entry.kind === "file" && onOpen({ root, rel: path }, true)}
            ondblclick={() => entry.kind === "file" && onOpen({ root, rel: path }, false)}
            oncontextmenu={(e) => openMenu(e, root, path, entry.kind === "dir" ? "dir" : "file")}
          >
            <span class="chevron" aria-hidden="true"
              >{#if entry.kind === "dir"}<Icon name={isOpen(root, path) ? "chevron-down" : "chevron-right"} size="sm" />{/if}</span
            >
            <FileIcon name={entry.name} folder={entry.kind === "dir"} open={entry.kind === "dir" && isOpen(root, path)} />
            <span class="label">{entry.name}{entry.kind === "link" ? " ↗" : ""}</span>
          </button>
          <button
            type="button"
            class="more"
            aria-label="Actions for {entry.name}"
            onclick={(e) => openMenu(e, root, path, entry.kind === "dir" ? "dir" : "file")}>⋯</button
          >
        </div>
      {/if}
      {#if confirming && confirming.root === root && confirming.rel === path}
        <div class="row confirm" style:--depth={depth}>
          <span>
            {confirming.count === null && entry.kind === "dir"
              ? "Counting…"
              : entry.kind === "dir"
                ? `Delete ${entry.name} and ${confirming.count} entr${confirming.count === 1 ? "y" : "ies"} in it?`
                : `Delete ${entry.name}?`}
          </span>
          <button type="button" class="danger" onclick={() => void commitDelete()}>Delete</button>
          <button type="button" onclick={() => (confirming = null)}>Cancel</button>
        </div>
      {/if}
      {#if entry.kind === "dir" && isOpen(root, path)}
        {@render folder(root, path, depth + 1)}
      {/if}
    {/each}
    {#if loaded.truncated}<div class="row note" style:--depth={depth}>…more entries than shown</div>{/if}
  {/if}
{/snippet}

<nav
  class="tree"
  aria-label="Files"
  onclickcapture={(e) => {
    if (!swallowClick) return;
    swallowClick = false;
    e.preventDefault();
    e.stopPropagation();
  }}
>
  {#each roots as root (root.id)}
    <div
      class="line"
      class:drop-target={isDropTarget(root.id, "")}
      data-drop-root={root.id}
      data-drop-dir=""
      data-folder=""
    >
      <button
        type="button"
        class="row root"
        style:--depth={0}
        title={root.path}
        onclick={() => toggle(root.id, "")}
        oncontextmenu={(e) => openMenu(e, root.id, "", "root")}
      >
        <span class="chevron" aria-hidden="true"
          ><Icon name={isOpen(root.id, "") ? "chevron-down" : "chevron-right"} size="sm" /></span
        >
        <span class="label">{root.label}</span>
      </button>
      <button
        type="button"
        class="more"
        aria-label="Actions for {root.label}"
        onclick={(e) => openMenu(e, root.id, "", "root")}>⋯</button
      >
    </div>
    {#if isOpen(root.id, "")}
      {@render folder(root.id, "", 1)}
    {/if}
  {/each}
</nav>

{#if drag}
  <!-- What is being dragged, beside the pointer; struck through where it cannot go. -->
  <div
    class="ghost"
    class:refused={dropping !== null && "refused" in dropping}
    style:left="{drag.x + 12}px"
    style:top="{drag.y + 8}px"
    aria-hidden="true"
  >
    {drag.name}
  </div>
{/if}

{#if menu}
  {@const m = menu}
  <div class="menu" role="menu" style:left="{m.x}px" style:top="{m.y}px">
    {#if m.kind !== "file"}
      <button type="button" role="menuitem" onclick={() => startNew("new-file", m.root, m.rel)}>New file</button>
      <button type="button" role="menuitem" onclick={() => startNew("new-dir", m.root, m.rel)}>New folder</button>
    {/if}
    {#if m.kind !== "root"}
      <button type="button" role="menuitem" onclick={() => startRename(m.root, m.rel)}>Rename</button>
      <button type="button" role="menuitem" class="danger" onclick={() => void startDelete(m.root, m.rel, m.kind === "dir")}
        >Delete…</button
      >
    {/if}
  </div>
{/if}

<style>
  .tree {
    display: flex;
    flex-direction: column;
    padding: var(--ax-space-1) 0;
    overflow: auto;
    font-size: var(--ax-font-size-sm);
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--ax-space-1);
    width: 100%;
    min-height: calc(var(--ax-font-size-sm) * 1.9);
    padding: 0 var(--ax-space-2) 0 calc(var(--ax-space-3) + var(--depth) * var(--ax-space-4));
    background: none;
    border: 0;
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: inherit;
    text-align: left;
    cursor: pointer;
  }

  /* A row and its ⋯, side by side: two buttons, never one inside the other. */
  .line {
    display: flex;
    align-items: center;
  }

  .line:hover {
    background: var(--ax-surface-2);
  }

  .line.active {
    background: var(--ax-accent-muted);
  }

  .row.root {
    color: var(--ax-text);
    font-weight: 600;
  }

  .row.ignored .label {
    color: var(--ax-text-muted);
  }

  .chevron {
    display: inline-flex;
    align-items: center;
    width: var(--ax-icon-sm);
    color: var(--ax-text-muted);
    flex-shrink: 0;
  }

  .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .more {
    visibility: hidden;
    padding: 0 var(--ax-space-2);
    background: none;
    border: 0;
    color: var(--ax-text-muted);
    cursor: pointer;
  }

  .line:hover .more,
  .more:focus-visible {
    visibility: visible;
  }

  .note {
    color: var(--ax-text-muted);
    cursor: default;
  }

  .note.error,
  .problem {
    color: var(--ax-danger);
    font-size: var(--ax-font-size-xs);
  }

  .name-input {
    flex: 1;
    min-width: 0;
    padding: 0 var(--ax-space-1);
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-accent);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: inherit;
  }

  .confirm {
    flex-wrap: wrap;
    gap: var(--ax-space-2);
    padding-top: var(--ax-space-1);
    padding-bottom: var(--ax-space-1);
    background: var(--ax-accent-muted);
    cursor: default;
  }

  .confirm span {
    flex-basis: 100%;
  }

  .confirm button,
  .menu button {
    padding: 0 var(--ax-space-2);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
    cursor: pointer;
  }

  .danger {
    color: var(--ax-danger);
  }

  .menu {
    position: fixed;
    z-index: calc(var(--ax-z-staging) + 1);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    padding: var(--ax-space-1);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    box-shadow: var(--ax-shadow-pop);
  }

  .menu button {
    border: 0;
    border-radius: var(--ax-radius-sm);
    background: none;
    text-align: left;
    font-size: var(--ax-font-size-sm);
  }

  .menu button:hover {
    background: var(--ax-accent-muted);
  }

  /* Moving (T11): the folder a drop goes into, the entry being dragged, and the ghost at the pointer. */
  .line.drop-target {
    background: var(--ax-accent-muted);
    box-shadow: inset 0 0 0 1px var(--ax-accent);
    border-radius: var(--ax-radius-sm);
  }

  .line.dragged {
    opacity: 0.5;
  }

  .ghost {
    position: fixed;
    z-index: var(--ax-z-dialog);
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-surface-3);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font-size: var(--ax-font-size-xs);
    pointer-events: none;
    white-space: nowrap;
  }

  .ghost.refused {
    color: var(--ax-text-muted);
    text-decoration: line-through;
  }
</style>
