/**
 * Keeps a pane's DOM where the dock wants it, without ever rebuilding it.
 *
 * The problem this solves, found live with three agents open: Svelte cannot
 * move a component between two `{#each}` blocks. The dock tree renders
 * recursively, so any structural change — docking to an edge, dragging a tab
 * into another group — changes which block a pane belongs to, and Svelte's
 * only way to honour that is to destroy the old instance and build a new one.
 * For a terminal pane that means the PTY is closed and the agent restarts. It
 * happened to *every* pane on screen, not just the one being dragged, because
 * inserting a split moves its neighbours a level deeper too.
 *
 * So the panes do not live in the tree. They are rendered once, flat, in a
 * store that is never reordered, and the tree contains empty slots. After each
 * layout change every pane is moved into its slot with `appendChild`, which
 * *relocates* a DOM node rather than recreating it — the component, its state,
 * its canvas and its PTY all carry on.
 *
 * The one thing to be careful about is size. A terminal measures itself with a
 * `ResizeObserver` and tells its PTY how many rows it has, so a pane must never
 * be parked anywhere that measures zero. The store therefore fills the dock
 * area and is hidden with `visibility`, exactly as an inactive tab is.
 */

/** The attribute a rendered pane carries, holding its tab id. */
export const PANE_ATTR = "data-ide-pane-for";

/** The attribute an empty slot in the tree carries, holding the same id. */
export const SLOT_ATTR = "data-ide-slot";

/**
 * Parks every pane back in the store, to be called **before** the tree is
 * rebuilt.
 *
 * Without this, `placePanes` alone is not enough: a pane sits inside a slot,
 * and when a layout change removes that slot — which happens on every
 * structural change, since Svelte rebuilds the part of the tree that moved —
 * the pane is torn out of the document along with it. Its component survives,
 * because it was never rendered in the tree, but its DOM node is detached: a
 * terminal then measures zero and tells its PTY it has no rows.
 *
 * So the dance is two-step. Park first (`$effect.pre`, before Svelte touches
 * the DOM), let the tree rebuild, then place (`$effect`, after). A pane is
 * only ever in the store or in a slot, never nowhere.
 */
export function parkPanes(root: ParentNode, store: Element): number {
  let parked = 0;
  for (const pane of root.querySelectorAll(`[${PANE_ATTR}]`)) {
    if (pane.parentElement === store) continue;
    store.appendChild(pane);
    parked += 1;
  }
  return parked;
}

/** Where a pane currently is, and where the layout says it should be. */
export interface PanePlacement {
  paneId: string;
  moved: boolean;
  /** No slot in the tree wants this pane right now. */
  orphaned: boolean;
}

/**
 * Moves every pane under `root` into the slot that names it.
 *
 * Returns one entry per pane, saying whether it had to move and whether it has
 * a home at all — a pane with no slot is left where it is rather than removed,
 * because "no slot right now" is a transient state during a layout change and
 * detaching would be exactly the destruction this module exists to avoid.
 *
 * Idempotent: a pane already in its slot is not touched, which matters because
 * re-appending a node that is already in place still costs a reflow, and a
 * terminal would see the resulting resize.
 */
export function placePanes(root: ParentNode): PanePlacement[] {
  const slots = new Map<string, Element>();
  for (const slot of root.querySelectorAll(`[${SLOT_ATTR}]`)) {
    const id = slot.getAttribute(SLOT_ATTR);
    // First slot wins. Two slots claiming one pane cannot happen from a valid
    // layout (a tab is in exactly one group) and picking deterministically
    // beats picking last.
    if (id && !slots.has(id)) slots.set(id, slot);
  }

  const placements: PanePlacement[] = [];
  for (const pane of root.querySelectorAll(`[${PANE_ATTR}]`)) {
    const paneId = pane.getAttribute(PANE_ATTR);
    if (!paneId) continue;
    const slot = slots.get(paneId);
    if (!slot) {
      placements.push({ paneId, moved: false, orphaned: true });
      continue;
    }
    const moved = pane.parentElement !== slot;
    if (moved) slot.appendChild(pane);
    placements.push({ paneId, moved, orphaned: false });
  }
  return placements;
}
