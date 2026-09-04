/**
 * `use:draggable` — pointer-event drag for canvas tiles.
 *
 * Nothing moves until the pointer travels `threshold` px, so plain clicks on
 * a module's buttons and inputs still register as clicks. Elements matching
 * `ignore` (form controls, links, resize handles, `[data-no-drag]`) never
 * start a drag; if `handle` is set, a drag only starts from inside a matching
 * descendant (the tile's slim top strip). Pointer capture is taken only once
 * the threshold is crossed, so the drag survives the pointer leaving the tile.
 */

import type { Action } from "svelte/action";

export interface DragDelta {
  dx: number;
  dy: number;
}

export interface DragOptions {
  /** Called once when the threshold is crossed. */
  onStart?: () => void;
  /** Called on every move after start, with the total offset since start. */
  onMove?: (delta: DragDelta) => void;
  /** Called on release with the final offset. Not called if no drag started. */
  onEnd?: (delta: DragDelta) => void;
  /** Movement in px before a press becomes a drag. */
  threshold?: number;
  /** Selector for descendants that must not start a drag. */
  ignore?: string;
  /** If set, a drag only starts from a press inside a matching descendant. */
  handle?: string;
}

export const DEFAULT_THRESHOLD_PX = 4;
export const DEFAULT_IGNORE = "button, input, textarea, select, a, [data-no-drag], .resize";

export const draggable: Action<HTMLElement, DragOptions> = (node, options) => {
  let opts: DragOptions = options ?? {};
  let pointerId: number | null = null;
  let startX = 0;
  let startY = 0;
  let dragging = false;

  function delta(e: PointerEvent): DragDelta {
    return { dx: e.clientX - startX, dy: e.clientY - startY };
  }

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0 || pointerId !== null) return;
    const ignore = opts.ignore ?? DEFAULT_IGNORE;
    if (e.target instanceof Element && e.target.closest(ignore)) return;
    if (opts.handle && !(e.target instanceof Element && e.target.closest(opts.handle))) return;
    pointerId = e.pointerId;
    startX = e.clientX;
    startY = e.clientY;
    dragging = false;
    node.addEventListener("pointermove", onPointerMove);
    node.addEventListener("pointerup", onPointerUp);
    node.addEventListener("pointercancel", onPointerUp);
  }

  function onPointerMove(e: PointerEvent) {
    if (e.pointerId !== pointerId) return;
    const d = delta(e);
    if (!dragging) {
      const threshold = opts.threshold ?? DEFAULT_THRESHOLD_PX;
      if (Math.hypot(d.dx, d.dy) < threshold) return;
      dragging = true;
      node.setPointerCapture(e.pointerId);
      opts.onStart?.();
    }
    e.preventDefault();
    opts.onMove?.(d);
  }

  function onPointerUp(e: PointerEvent) {
    if (e.pointerId !== pointerId) return;
    node.removeEventListener("pointermove", onPointerMove);
    node.removeEventListener("pointerup", onPointerUp);
    node.removeEventListener("pointercancel", onPointerUp);
    if (dragging) {
      node.releasePointerCapture(e.pointerId);
      opts.onEnd?.(delta(e));
    }
    pointerId = null;
    dragging = false;
  }

  node.addEventListener("pointerdown", onPointerDown);

  return {
    update(next) {
      opts = next ?? {};
    },
    destroy() {
      node.removeEventListener("pointerdown", onPointerDown);
      node.removeEventListener("pointermove", onPointerMove);
      node.removeEventListener("pointerup", onPointerUp);
      node.removeEventListener("pointercancel", onPointerUp);
    },
  };
};
