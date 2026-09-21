/**
 * The IDE's dock layout: the tree, and every operation the view performs on it.
 *
 * No DOM, no Tauri, no Svelte — the same cut as `core/kanban.ts`. `IdeView.svelte`
 * draws what is here and computes pixel geometry; every rule that could be wrong
 * lives in this file where a test can reach it. That order (model first, surface
 * second) is the one that worked for the terminal engine.
 *
 * The shape is the one settled in the plan: a tree of `Split` and `TabGroup`
 * nodes. Five decisions are load-bearing enough to name:
 *
 * * **Every operation is pure.** It returns a new tree and never writes into the
 *   one handed in; unchanged subtrees are shared rather than copied. Treat a
 *   `Layout` as immutable — mutating one in place would silently change the tree
 *   a previous operation returned.
 * * **Sizes are fractions of their split, summing to 1**, never pixels. Resizing
 *   the window, or a 21:9 monitor after a 16:9 one, then needs no layout rewrite.
 * * **The active tab is an id, not an index.** An index points at a different tab
 *   the moment a neighbour is closed or moved, and does so silently.
 * * **The tree is normalised after every operation**: empty groups disappear, a
 *   split left with one child collapses into it, and a split nested directly in a
 *   split of the same direction is flattened into its parent. Without the last
 *   one, repeated docking builds an ever-deeper tree that draws identically —
 *   correct, but impossible to reason about, and it is what the stored layout
 *   would grow to over months.
 * * **The root always exists.** An IDE with everything closed is one empty
 *   `TabGroup`, so there is still something to drop a pane onto.
 *
 * `PaneTab.kind` is a plain string on purpose: this module is a tree, not a
 * component registry, and a layout stored by a newer version must not be emptied
 * by an older one that has never heard of its pane kinds. `IdeView` maps the
 * kind to a component and shows a placeholder for one it does not know.
 */

/** The axis a split divides along: `row` = side by side, `col` = stacked. */
export type SplitDir = "row" | "col";

/** One tab in a group — what the view draws inside a pane. */
export interface PaneTab {
  /** Unique across the whole tree; supplied by the caller (agent id, terminal instance id, …). */
  id: string;
  /** Which pane component draws it. Opaque here — see the module doc. */
  kind: string;
  /** Label on the tab. */
  title: string;
  /** The pane's own state (file path, module instance, …). Carried, never read or written. */
  config?: Record<string, unknown>;
}

/** A stack of tabs, one of them visible. The only node that holds content. */
export interface TabGroup {
  type: "tabs";
  id: string;
  tabs: PaneTab[];
  /** Id of the visible tab; `null` only while the group is empty, which only the root can be. */
  active: string | null;
}

/** A row or column of children, each taking its fraction of the split. */
export interface Split {
  type: "split";
  id: string;
  dir: SplitDir;
  /** Always at least two: a one-child split collapses during normalisation. */
  children: LayoutNode[];
  /** One fraction per child, in the same order, summing to 1. */
  sizes: number[];
}

export type LayoutNode = Split | TabGroup;

/** A whole dock layout. `root` is never absent — see the module doc. */
export interface Layout {
  root: LayoutNode;
}

/** Where a dragged pane lands: an edge of the target splits it, the centre joins its tabs. */
export type DockSide = "center" | "left" | "right" | "top" | "bottom";

export interface DockTarget {
  /**
   * The node being dropped onto. For `center` it must be a `TabGroup`; for an
   * edge it can be any node, including `layout.root` — that is how a pane docks
   * against the outer edge of the whole view.
   */
  nodeId: string;
  side: DockSide;
  /**
   * `center` only: the slot among the group's *other* tabs, counted with the
   * moved tab left out, matching `core/kanban.ts`'s `dropTarget`. Omitted means
   * last.
   */
  index?: number;
}

/** Where a tab sits, as {@link findTab} reports it. */
export interface TabLocation {
  group: TabGroup;
  tab: PaneTab;
  /** The tab's position within `group.tabs`. */
  index: number;
}

/** Schema version written into a serialised layout; see {@link parseLayout}. */
export const LAYOUT_VERSION = 1;

/**
 * The smallest fraction of its split a pane may be dragged to.
 *
 * Below roughly this, a pane is a sliver with no reachable content and no
 * grabbable splitter — a state the user can enter with one careless drag and
 * cannot leave with another. A drag that would push a neighbour under it stops
 * at it instead.
 */
export const MIN_PANE_FRACTION = 0.08;

const isStr = (v: unknown): v is string => typeof v === "string" && v.length > 0;

function newNodeId(): string {
  return crypto.randomUUID();
}

/** True for a split node — the narrowing the view needs when walking the tree. */
export function isSplit(node: LayoutNode): node is Split {
  return node.type === "split";
}

/** True for a tab group. */
export function isGroup(node: LayoutNode): node is TabGroup {
  return node.type === "tabs";
}

/** A layout holding nothing: one empty group, ready to be dropped onto. */
export function emptyLayout(): Layout {
  return { root: { type: "tabs", id: newNodeId(), tabs: [], active: null } };
}

/** A layout holding exactly these tabs in one group, the first one visible. */
export function singleGroupLayout(tabs: PaneTab[]): Layout {
  if (tabs.length === 0) return emptyLayout();
  const copies = tabs.map((tab) => ({ ...tab }));
  return { root: { type: "tabs", id: newNodeId(), tabs: copies, active: copies[0].id } };
}

/** Every tab in the tree, in reading order (left to right, top to bottom). */
export function allTabs(layout: Layout): PaneTab[] {
  const out: PaneTab[] = [];
  walk(layout.root, (node) => {
    if (isGroup(node)) out.push(...node.tabs);
  });
  return out;
}

/** Every group in the tree, in reading order. */
export function allGroups(layout: Layout): TabGroup[] {
  const out: TabGroup[] = [];
  walk(layout.root, (node) => {
    if (isGroup(node)) out.push(node);
  });
  return out;
}

/** Where a tab sits, or `null` if no tab has that id. */
export function findTab(layout: Layout, tabId: string): TabLocation | null {
  for (const group of allGroups(layout)) {
    const index = group.tabs.findIndex((tab) => tab.id === tabId);
    if (index !== -1) return { group, tab: group.tabs[index], index };
  }
  return null;
}

/**
 * The node with that id, or `null`.
 *
 * Depth-first, first match — deliberately the same order `replaceNode` stops
 * at. Ids are unique in a tree this module built, but a stored layout is only
 * as good as whoever last edited it; if two nodes ever did share an id, a
 * lookup and the write that follows it must at least agree on which one they
 * mean.
 */
export function findNode(layout: Layout, nodeId: string): LayoutNode | null {
  return firstNode(layout.root, nodeId);
}

/**
 * Adds a tab to the layout at a dock target.
 *
 * Unchanged when the id is already in the tree (ids are the handle for every
 * other operation, so a duplicate would make all of them ambiguous), when the
 * target node does not exist, or when a `center` drop names a split.
 */
export function addTab(layout: Layout, tab: PaneTab, target: DockTarget): Layout {
  if (findTab(layout, tab.id)) return layout;
  const root = insertAt(layout.root, { ...tab }, target);
  return root ? { root: finalize(root) } : layout;
}

/**
 * Moves a tab that is already in the layout to a dock target.
 *
 * Moving a group's only tab onto that same group leaves the layout untouched —
 * identical object, identical ids — on every side, because that is what the
 * user did: they dropped it back where it came from.
 */
export function moveTab(layout: Layout, tabId: string, target: DockTarget): Layout {
  const hit = findTab(layout, tabId);
  if (!hit) return layout;

  // A group's only tab, dropped back on that same group: nothing to do, on any
  // side. Without this the general path below still *renders* the same picture,
  // but gets there by emptying the group, wrapping it in a split and collapsing
  // it again — leaving a group with a brand-new id where the user sees the one
  // they never moved. CP2 keys drag and transition state off those ids.
  if (target.nodeId === hit.group.id && hit.group.tabs.length === 1) return layout;

  // Reordering inside one group is its own case: detaching first would shift the
  // very indices the caller computed against the order on screen.
  if (target.side === "center" && target.nodeId === hit.group.id) {
    const root = replaceNode(layout.root, hit.group.id, (node) =>
      isGroup(node) ? withTabAt(withoutTab(node, tabId), hit.tab, target.index) : node,
    );
    return root ? { root: finalize(root) } : layout;
  }

  // Deliberately not finalised in between: the source group may now be empty and
  // still be the node being docked onto. Normalising here would delete it first.
  const detached = replaceNode(layout.root, hit.group.id, (node) => (isGroup(node) ? withoutTab(node, tabId) : node));
  if (!detached) return layout;
  const inserted = insertAt(detached, hit.tab, target);
  return inserted ? { root: finalize(inserted) } : layout;
}

/**
 * Closes a tab.
 *
 * The tab that slides into its place becomes active, or the new last one if it
 * was the last. An emptied group disappears, along with any split left with a
 * single child; closing the final tab leaves the empty root group behind rather
 * than nothing.
 */
export function closeTab(layout: Layout, tabId: string): Layout {
  const hit = findTab(layout, tabId);
  if (!hit) return layout;
  const root = replaceNode(layout.root, hit.group.id, (node) => (isGroup(node) ? withoutTab(node, tabId) : node));
  return root ? { root: finalize(root) } : layout;
}

/**
 * Replaces a tab's `config` — the pane's own state on its way to `layout_json`.
 *
 * A pane has no row in `dashboard.json` the way a canvas tile does, so this is
 * where a module's config lands when it is hosted in the IDE (see
 * `moduleAdapter.ts`). The object is stored as given; the layout never reads
 * into it.
 */
export function setTabConfig(layout: Layout, tabId: string, config: Record<string, unknown>): Layout {
  const hit = findTab(layout, tabId);
  if (!hit) return layout;
  const root = replaceNode(layout.root, hit.group.id, (node) => ({
    ...(node as TabGroup),
    tabs: (node as TabGroup).tabs.map((tab) => (tab.id === tabId ? { ...tab, config } : tab)),
  }));
  return root ? { root: finalize(root) } : layout;
}

/** Makes a tab the visible one in its group. Unchanged if there is no such tab. */
export function activateTab(layout: Layout, tabId: string): Layout {
  const hit = findTab(layout, tabId);
  if (!hit || hit.group.active === tabId) return layout;
  const root = replaceNode(layout.root, hit.group.id, (node) => ({ ...(node as TabGroup), active: tabId }));
  return root ? { root: finalize(root) } : layout;
}

/**
 * Drags the divider after child `boundary` of a split.
 *
 * Only the two children touching that divider change; everything else in the
 * split keeps its fraction, which is what a splitter does on screen. `fraction`
 * is the new share of the child *before* the divider, clamped so neither side
 * falls under {@link MIN_PANE_FRACTION}.
 */
export function resizeSplit(layout: Layout, splitId: string, boundary: number, fraction: number): Layout {
  const node = findNode(layout, splitId);
  if (!node || !isSplit(node)) return layout;
  if (!Number.isFinite(fraction) || !Number.isInteger(boundary)) return layout;
  if (boundary < 0 || boundary + 1 >= node.children.length) return layout;

  const pair = node.sizes[boundary] + node.sizes[boundary + 1];
  const min = Math.min(MIN_PANE_FRACTION, pair / 2);
  const first = Math.min(Math.max(fraction, min), pair - min);
  const sizes = [...node.sizes];
  sizes[boundary] = first;
  sizes[boundary + 1] = pair - first;

  const root = replaceNode(layout.root, splitId, (found) => ({ ...(found as Split), sizes }));
  return root ? { root: finalize(root) } : layout;
}

/**
 * Replaces a split's sizes wholesale — for restoring a drag or setting a preset.
 *
 * Wrong-length, negative or non-finite input is repaired the same way a stored
 * layout's is, rather than refused: see {@link parseLayout}.
 */
export function setSplitSizes(layout: Layout, splitId: string, sizes: number[]): Layout {
  const node = findNode(layout, splitId);
  if (!node || !isSplit(node)) return layout;
  const fitted = normalizeSizes(sizes, node.children.length);
  const root = replaceNode(layout.root, splitId, (found) => ({ ...(found as Split), sizes: fitted }));
  return root ? { root: finalize(root) } : layout;
}

/** The layout as the string stored in a project's `layout_json`. */
export function serializeLayout(layout: Layout): string {
  return JSON.stringify({ version: LAYOUT_VERSION, root: layout.root });
}

/**
 * Reads back a stored layout, repairing what it can and refusing what it cannot.
 *
 * Takes the JSON string from `layout_json` or an already-parsed value. Anything
 * malformed is dropped rather than trusted: unknown node types, tabs without an
 * id or kind, a second tab reusing an id, sizes that do not match the children.
 * Returns `null` when nothing usable survives, so the caller can fall back to
 * its starting layout — that case is a hand-edited or truncated row, and an IDE
 * that silently opens empty would look like data loss.
 *
 * The stored `version` is written but not branched on, the same way
 * `core/persist.ts` treats its own: with one version there is nothing to
 * branch. The first change to the tree's shape is what has to add that branch —
 * there is none to fall back on.
 */
export function parseLayout(raw: unknown): Layout | null {
  let value = raw;
  if (typeof value === "string") {
    try {
      value = JSON.parse(value);
    } catch {
      return null;
    }
  }
  if (typeof value !== "object" || value === null) return null;

  const record = value as Record<string, unknown>;
  const root = sanitizeNode(record.root, { nodes: new Set<string>(), tabs: new Set<string>() });
  if (!root) return null;
  const normalized = normalizeNode(root);
  return { root: normalized ?? root };
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

function walk(node: LayoutNode, visit: (node: LayoutNode) => void): void {
  visit(node);
  if (isSplit(node)) for (const child of node.children) walk(child, visit);
}

function firstNode(node: LayoutNode, nodeId: string): LayoutNode | null {
  if (node.id === nodeId) return node;
  if (!isSplit(node)) return null;
  for (const child of node.children) {
    const hit = firstNode(child, nodeId);
    if (hit) return hit;
  }
  return null;
}

/**
 * Rebuilds the path down to `nodeId` with `make`'s result in its place.
 *
 * `null` means no node carries that id. Nodes off the path are shared, not
 * copied — the tree is immutable, so sharing them is safe and keeps a deep
 * layout cheap to update.
 */
function replaceNode(node: LayoutNode, nodeId: string, make: (found: LayoutNode) => LayoutNode): LayoutNode | null {
  if (node.id === nodeId) return make(node);
  if (!isSplit(node)) return null;
  for (let i = 0; i < node.children.length; i++) {
    const replaced = replaceNode(node.children[i], nodeId, make);
    if (!replaced) continue;
    const children = [...node.children];
    children[i] = replaced;
    return { ...node, children };
  }
  return null;
}

/** The dock target's node with the tab inserted; `null` if the target is unusable. */
function insertAt(root: LayoutNode, tab: PaneTab, target: DockTarget): LayoutNode | null {
  if (target.side === "center") {
    const node = findNode({ root }, target.nodeId);
    if (!node || !isGroup(node)) return null;
    return replaceNode(root, target.nodeId, (found) => withTabAt(found as TabGroup, tab, target.index));
  }
  const dir: SplitDir = target.side === "left" || target.side === "right" ? "row" : "col";
  const before = target.side === "left" || target.side === "top";
  return replaceNode(root, target.nodeId, (node) => {
    const fresh: TabGroup = { type: "tabs", id: newNodeId(), tabs: [tab], active: tab.id };
    return {
      type: "split",
      id: newNodeId(),
      dir,
      children: before ? [fresh, node] : [node, fresh],
      sizes: [0.5, 0.5],
    };
  });
}

/** The group with `tab` inserted at `index` among its current tabs, and made active. */
function withTabAt(group: TabGroup, tab: PaneTab, index?: number): TabGroup {
  const tabs = [...group.tabs];
  const at = index === undefined ? tabs.length : Math.min(Math.max(index, 0), tabs.length);
  tabs.splice(at, 0, tab);
  return { ...group, tabs, active: tab.id };
}

/** The group without that tab, with the succession rule from {@link closeTab} applied. */
function withoutTab(group: TabGroup, tabId: string): TabGroup {
  const index = group.tabs.findIndex((tab) => tab.id === tabId);
  if (index === -1) return group;
  const tabs = group.tabs.filter((tab) => tab.id !== tabId);
  if (group.active !== tabId) return { ...group, tabs };
  const successor = tabs[index] ?? tabs[tabs.length - 1] ?? null;
  return { ...group, tabs, active: successor ? successor.id : null };
}

/** Normalisation, with the guarantee that the root survives as an empty group. */
function finalize(root: LayoutNode): LayoutNode {
  const normalized = normalizeNode(root);
  if (normalized) return normalized;
  // Everything was closed. Keep the root group's id if it is one, so a view
  // holding a drop target for it does not lose its handle.
  return isGroup(root) ? { ...root, tabs: [], active: null } : emptyLayout().root;
}

/**
 * Drops empty groups, collapses one-child splits, flattens same-direction
 * splits into their parent and fixes every `active` and size along the way.
 * `null` means the whole subtree held nothing.
 */
function normalizeNode(node: LayoutNode): LayoutNode | null {
  if (isGroup(node)) {
    if (node.tabs.length === 0) return null;
    const active = pickActive(node.tabs, node.active);
    return active === node.active ? node : { ...node, active };
  }

  const children: LayoutNode[] = [];
  const sizes: number[] = [];
  node.children.forEach((child, i) => {
    const normalized = normalizeNode(child);
    if (!normalized) return;
    const share = node.sizes[i];
    if (isSplit(normalized) && normalized.dir === node.dir) {
      // A row inside a row is the same picture with a deeper tree. Its children
      // move up, each keeping its share of the share its parent had.
      normalized.children.forEach((grandchild, j) => {
        children.push(grandchild);
        sizes.push(share * normalized.sizes[j]);
      });
      return;
    }
    children.push(normalized);
    sizes.push(share);
  });

  if (children.length === 0) return null;
  if (children.length === 1) return children[0];
  return { ...node, children, sizes: normalizeSizes(sizes, children.length) };
}

/**
 * Fractions for `count` children that are finite, sum to 1 and each reach
 * {@link MIN_PANE_FRACTION}.
 *
 * A missing or unusable entry gets the average of the usable ones rather than
 * zero: zero would be a child that exists, cannot be seen and cannot be dragged
 * back into view. The floor is applied here rather than only in `resizeSplit`
 * because this is the one function every size in the tree passes through — a
 * preset, a flattened split and a layout read back from `layout_json` would
 * otherwise each be able to reintroduce the sliver that the floor exists to
 * prevent, and the stored one would do it on every start.
 */
function normalizeSizes(sizes: number[], count: number): number[] {
  const usable = sizes.slice(0, count).map((size) => (Number.isFinite(size) && size > 0 ? size : null));
  while (usable.length < count) usable.push(null);

  const known = usable.filter((size): size is number => size !== null);
  const fallback = known.length > 0 ? known.reduce((a, b) => a + b, 0) / known.length : 1;
  const filled = usable.map((size) => size ?? fallback);
  const total = filled.reduce((a, b) => a + b, 0);
  return applyFloor(
    filled.map((size) => size / total),
    count,
  );
}

/**
 * Lifts every share to the floor, taking what that costs from the others in
 * proportion.
 *
 * The floor is {@link MIN_PANE_FRACTION}, or an even share when a split holds
 * so many children that the constant could not be met by all of them. Lifting
 * one child can push another under the floor in turn, so this repeats — at most
 * once per child, since each pass pins at least one more of them.
 */
function applyFloor(shares: number[], count: number): number[] {
  const floor = Math.min(MIN_PANE_FRACTION, 1 / count);
  const out = [...shares];

  for (let pass = 0; pass < count; pass++) {
    const pinned = new Set(out.map((share, i) => (share < floor ? i : -1)).filter((i) => i >= 0));
    if (pinned.size === 0) break;

    const room = 1 - pinned.size * floor;
    const rest = out.reduce((sum, share, i) => (pinned.has(i) ? sum : sum + share), 0);
    const free = count - pinned.size;
    for (let i = 0; i < out.length; i++) {
      if (pinned.has(i)) out[i] = floor;
      else out[i] = rest > 0 ? (out[i] / rest) * room : room / free;
    }
  }
  return out;
}

/** The active tab: the requested one if it is still there, else the first. */
function pickActive(tabs: PaneTab[], requested: string | null): string {
  return requested !== null && tabs.some((tab) => tab.id === requested) ? requested : tabs[0].id;
}

/** The ids already handed out while reading one stored layout. */
interface SeenIds {
  nodes: Set<string>;
  tabs: Set<string>;
}

/**
 * One node from stored JSON, or `null` if it cannot be trusted.
 *
 * A node id that a previous node already used is replaced by a fresh one rather
 * than kept, for the same reason a repeated tab id drops the tab: every
 * operation addresses nodes by id, so two claimants make all of them ambiguous.
 * Dropping the node would throw away panes the user still has; renaming it only
 * costs the stored dock target of a node nobody can reach anyway.
 */
function sanitizeNode(raw: unknown, seen: SeenIds): LayoutNode | null {
  if (typeof raw !== "object" || raw === null) return null;
  const record = raw as Record<string, unknown>;
  const id = isStr(record.id) && !seen.nodes.has(record.id) ? record.id : newNodeId();
  seen.nodes.add(id);

  if (record.type === "tabs") {
    const tabs = Array.isArray(record.tabs) ? record.tabs.map((tab) => sanitizeTab(tab, seen)) : [];
    const kept = tabs.filter((tab): tab is PaneTab => tab !== null);
    if (kept.length === 0) return null;
    return { type: "tabs", id, tabs: kept, active: pickActive(kept, isStr(record.active) ? record.active : null) };
  }

  if (record.type === "split") {
    const dir: SplitDir = record.dir === "col" ? "col" : "row";
    const rawChildren = Array.isArray(record.children) ? record.children : [];
    const rawSizes = Array.isArray(record.sizes) ? record.sizes : [];
    const children: LayoutNode[] = [];
    const sizes: number[] = [];
    rawChildren.forEach((child, i) => {
      const node = sanitizeNode(child, seen);
      if (!node) return;
      children.push(node);
      sizes.push(typeof rawSizes[i] === "number" ? (rawSizes[i] as number) : NaN);
    });
    if (children.length === 0) return null;
    if (children.length === 1) return children[0];
    return { type: "split", id, dir, children, sizes: normalizeSizes(sizes, children.length) };
  }

  return null;
}

/** One tab from stored JSON, or `null` if it lacks an id or kind, or reuses an id. */
function sanitizeTab(raw: unknown, seen: SeenIds): PaneTab | null {
  if (typeof raw !== "object" || raw === null) return null;
  const record = raw as Record<string, unknown>;
  if (!isStr(record.id) || !isStr(record.kind) || seen.tabs.has(record.id)) return null;
  seen.tabs.add(record.id);
  const tab: PaneTab = { id: record.id, kind: record.kind, title: isStr(record.title) ? record.title : record.kind };
  if (typeof record.config === "object" && record.config !== null && !Array.isArray(record.config)) {
    tab.config = record.config as Record<string, unknown>;
  }
  return tab;
}
