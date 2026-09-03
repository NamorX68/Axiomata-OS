/**
 * `use:resizable` — attach to a resize handle. The handle's `dir` says which
 * edges move: `"e"` (width), `"s"` (height) or `"se"` (both). Unlike drag
 * there is no click threshold: a handle has no other job, so capture starts
 * on press. Reports the size delta since press; the tile clamps and commits.
 */

import type { Action } from "svelte/action";

export type ResizeDir = "e" | "s" | "se";

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
    return {
      dw: opts.dir === "s" ? 0 : dx,
      dh: opts.dir === "e" ? 0 : dy,
    };
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
