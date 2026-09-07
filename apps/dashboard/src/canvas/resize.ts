/**
 * `use:resizable` — attach to a resize handle. The handle's `dir` says which
 * edge(s) move: `"e"`/`"w"` (width, growing right/left), `"s"`/`"n"`
 * (height, growing down/up), or `"se"` (both, growing right+down — the
 * canvas tile corner handle). `"w"`/`"n"` are for a panel anchored on the
 * opposite side (`StagingLayer`'s right-side panel: dragging its *left*
 * edge left, or its *top* edge up, both grow it, so those two report a
 * positive delta for a *negative* pointer movement). Unlike drag there is no
 * click threshold: a handle has no other job, so capture starts on press.
 * Reports the size delta since press; the caller clamps and commits.
 */

import type { Action } from "svelte/action";

export type ResizeDir = "e" | "s" | "se" | "w" | "n";

export interface ResizeDelta {
  dw: number;
  dh: number;
}

export interface ResizeOptions {
  dir: ResizeDir;
  onStart?: () => void;
  onMove?: (delta: ResizeDelta) => void;
  onEnd?: (delta: ResizeDelta) => void;
}

export const resizable: Action<HTMLElement, ResizeOptions> = (node, options) => {
  let opts = options;
  let pointerId: number | null = null;
  let startX = 0;
  let startY = 0;

  function delta(e: PointerEvent): ResizeDelta {
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    const dw = opts.dir === "e" || opts.dir === "se" ? dx : opts.dir === "w" ? -dx : 0;
    const dh = opts.dir === "s" || opts.dir === "se" ? dy : opts.dir === "n" ? -dy : 0;
    return { dw, dh };
  }

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0 || pointerId !== null) return;
    pointerId = e.pointerId;
    startX = e.clientX;
    startY = e.clientY;
    node.setPointerCapture(e.pointerId);
    node.addEventListener("pointermove", onPointerMove);
    node.addEventListener("pointerup", onPointerUp);
    node.addEventListener("pointercancel", onPointerUp);
    e.preventDefault();
    e.stopPropagation();
    opts.onStart?.();
  }

  function onPointerMove(e: PointerEvent) {
    if (e.pointerId !== pointerId) return;
    e.preventDefault();
    opts.onMove?.(delta(e));
  }

  function onPointerUp(e: PointerEvent) {
    if (e.pointerId !== pointerId) return;
    node.releasePointerCapture(e.pointerId);
    node.removeEventListener("pointermove", onPointerMove);
    node.removeEventListener("pointerup", onPointerUp);
    node.removeEventListener("pointercancel", onPointerUp);
    pointerId = null;
    opts.onEnd?.(delta(e));
  }

  node.addEventListener("pointerdown", onPointerDown);

  return {
    update(next) {
      opts = next;
    },
    destroy() {
      node.removeEventListener("pointerdown", onPointerDown);
      node.removeEventListener("pointermove", onPointerMove);
      node.removeEventListener("pointerup", onPointerUp);
      node.removeEventListener("pointercancel", onPointerUp);
    },
  };
};
