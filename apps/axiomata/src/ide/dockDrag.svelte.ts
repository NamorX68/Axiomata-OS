/**
 * Dragging a tab or a divider in a dock (`docs/plans/editor-look.md`, LK2; M7.1 CP2) — one
 * implementation for both views that show a dock, the IDE's and the file app's. Each used to carry
 * its own copy of this state machine; what differs between them is only *what a move does to their
 * layout*, which the owner supplies as callbacks.
 *
 * * **Geometry is measured once, when the drag starts**, from the DOM (`data-ide-group`,
 *   `data-ide-tabbar`, `data-ide-tab`); the pointer then only moves over rectangles.
 * * **A press becomes a drag only after it moved a few pixels**, so a click on a tab stays a click.
 * * **Every way out of a drag ends it**: a drop, `pointercancel`, a window that lost focus
 *   ({@link DockDrag.abandon}). While a tab is being dragged the panes ignore the pointer, so a drag
 *   that never ends would leave the view unclickable.
 */

import {
  ROOT_NODE_ID,
  crossedDragThreshold,
  dividerFraction,
  dropTarget,
  splitFractionAt,
  type GroupGeometry,
  type Rect,
} from "./dock";
import { findNode, isSplit, type DockTarget, type Layout, type SplitDir } from "./layout";

export interface DockDragOptions {
  /** The element holding the dock: the drag is measured against it. */
  dockEl: () => HTMLElement | undefined;
  /** The layout as it is now. */
  layout: () => Layout;
  /** A tab was pressed (before a drag starts): bring it forward. */
  onPress: (tabId: string) => void;
  /** A tab was dropped on `target` (its `nodeId` already resolved for the root). */
  onMove: (tabId: string, target: DockTarget) => void;
  /** A divider was dragged: the boundary `boundary` of split `splitId` now sits at `fraction`. */
  onResize: (splitId: string, boundary: number, fraction: number) => void;
}

function rectOf(el: Element): Rect {
  const r = el.getBoundingClientRect();
  return { x: r.left, y: r.top, w: r.width, h: r.height };
}

export class DockDrag {
  /** The tab being dragged, once the pointer has moved far enough. */
  draggingTab = $state<string | null>(null);
  /** Where a drop would land. */
  hint = $state<DockTarget | null>(null);

  private pending: { tabId: string; pointerId: number; x: number; y: number } | null = null;
  private snapshot: { root: Rect; groups: GroupGeometry[] } | null = null;
  private divider: { splitId: string; boundary: number; pointerId: number; rect: Rect; dir: SplitDir } | null = null;

  constructor(private readonly options: DockDragOptions) {}

  /** The side of the whole dock a drop would dock to, for the highlight along its edge. */
  get rootHint(): DockTarget["side"] | null {
    return this.hint && this.hint.nodeId === ROOT_NODE_ID ? this.hint.side : null;
  }

  /**
   * A tab was pressed: it comes forward, and moving the pointer on may turn this into a drag.
   * Returns whether the press was taken (a right-click, or a drag already under way, is not).
   */
  startTab(tabId: string, event: PointerEvent): boolean {
    // A second pointer must not take over a drag already under way, and a right-click is not a drag at all.
    if (event.button !== 0 || this.pending || this.divider) return false;
    this.options.onPress(tabId);
    this.pending = { tabId, pointerId: event.pointerId, x: event.clientX, y: event.clientY };
    // On `window`, not the tab: the pointer spends the drag over other panes, and a terminal's canvas
    // would otherwise swallow the moves.
    window.addEventListener("pointermove", this.onTabMove);
    window.addEventListener("pointerup", this.onTabUp);
    window.addEventListener("pointercancel", this.endTab);
    return true;
  }

  /** A divider was pressed. */
  startDivider(splitId: string, boundary: number, event: PointerEvent): void {
    if (event.button !== 0 || this.pending || this.divider) return;
    const el = (event.currentTarget as HTMLElement | null)?.closest("[data-ide-split]");
    const node = findNode(this.options.layout(), splitId);
    if (!el || !node || !isSplit(node)) return;
    this.divider = { splitId, boundary, pointerId: event.pointerId, rect: rectOf(el), dir: node.dir };
    window.addEventListener("pointermove", this.onDividerMove);
    window.addEventListener("pointerup", this.onDividerUp);
    window.addEventListener("pointercancel", this.endDivider);
    event.preventDefault();
  }

  /** The window lost the pointer to something outside the page: end whatever drag is going. */
  abandon = (): void => {
    this.endTab();
    this.endDivider();
  };

  /**
   * The dragged tab is left out of every tab bar, because a drop index counts positions among the
   * *other* tabs.
   */
  private measure(draggedId: string): { root: Rect; groups: GroupGeometry[] } | null {
    const dockEl = this.options.dockEl();
    if (!dockEl) return null;
    const groups: GroupGeometry[] = [];
    for (const el of dockEl.querySelectorAll<HTMLElement>("[data-ide-group]")) {
      const bar = el.querySelector<HTMLElement>("[data-ide-tabbar]");
      if (!bar || !el.dataset.ideGroup) continue;
      groups.push({
        nodeId: el.dataset.ideGroup,
        rect: rectOf(el),
        tabBar: rectOf(bar),
        tabs: [...bar.querySelectorAll<HTMLElement>("[data-ide-tab]")]
          .filter((tab) => tab.dataset.ideTab !== draggedId)
          .map(rectOf),
      });
    }
    return { root: rectOf(dockEl), groups };
  }

  private onTabMove = (event: PointerEvent): void => {
    const pending = this.pending;
    if (!pending || event.pointerId !== pending.pointerId) return;
    if (!this.draggingTab) {
      if (!crossedDragThreshold(pending, event.clientX, event.clientY)) return;
      this.snapshot = this.measure(pending.tabId);
      if (!this.snapshot) return;
      this.draggingTab = pending.tabId;
    }
    if (this.snapshot) this.hint = dropTarget(this.snapshot.root, this.snapshot.groups, event.clientX, event.clientY);
  };

  private onTabUp = (event: PointerEvent): void => {
    if (this.pending && event.pointerId !== this.pending.pointerId) return;
    if (this.draggingTab && this.hint) {
      // `dropTarget` works from rectangles and cannot know the root's id.
      const nodeId = this.hint.nodeId === ROOT_NODE_ID ? this.options.layout().root.id : this.hint.nodeId;
      this.options.onMove(this.draggingTab, { ...this.hint, nodeId });
    }
    this.endTab();
  };

  private endTab = (): void => {
    this.pending = null;
    this.snapshot = null;
    this.draggingTab = null;
    this.hint = null;
    window.removeEventListener("pointermove", this.onTabMove);
    window.removeEventListener("pointerup", this.onTabUp);
    window.removeEventListener("pointercancel", this.endTab);
  };

  private onDividerMove = (event: PointerEvent): void => {
    const divider = this.divider;
    if (!divider || event.pointerId !== divider.pointerId) return;
    const node = findNode(this.options.layout(), divider.splitId);
    if (!node || !isSplit(node)) return;
    const along = splitFractionAt(divider.rect, divider.dir, event.clientX, event.clientY);
    this.options.onResize(divider.splitId, divider.boundary, dividerFraction(node.sizes, divider.boundary, along));
  };

  private onDividerUp = (event: PointerEvent): void => {
    if (this.divider && event.pointerId !== this.divider.pointerId) return;
    this.endDivider();
  };

  private endDivider = (): void => {
    this.divider = null;
    window.removeEventListener("pointermove", this.onDividerMove);
    window.removeEventListener("pointerup", this.onDividerUp);
    window.removeEventListener("pointercancel", this.endDivider);
  };
}
