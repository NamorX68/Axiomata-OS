/**
 * `use:resizable` — attach to a resize handle. The handle's `dir` says which
 * edge(s) move: `"e"`/`"w"` (width, growing right/left), `"s"`/`"n"`
 * (height, growing down/up), `"se"` (both, growing right+down — the
 * canvas tile corner handle), `"sw"` (both, growing left+down — the tile's
 * other lower corner, for a tile at the screen's right edge) or `"ne"`/`"nw"`
 * (both, growing up and right/left — a window's upper corners). `"w"`/`"n"` are for an edge anchored on the
 * opposite side (e.g. `StagingLayer`'s staged panel: dragging its *left*
 * edge left, or its *top* edge up, both grow it, so those two report a
 * positive delta for a *negative* pointer movement). Unlike drag there is no
 * click threshold: a handle has no other job, so capture starts on press.
 * Reports the size delta since press; the caller clamps and commits.
 */

import type { Action } from "svelte/action";

export type ResizeDir = "e" | "s" | "se" | "w" | "n" | "sw" | "ne" | "nw";

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

/** The growth a pointer offset (`dx`, `dy`, screen px) means for a handle on side(s) `dir`: an edge on the right or
 *  bottom grows with the pointer, one on the left or top against it, and an edge the handle does not touch not at all. */
export function resizeDelta(dir: ResizeDir, dx: number, dy: number): ResizeDelta {
  const dw = dir.includes("e") ? dx : dir.includes("w") ? -dx : 0;
  const dh = dir.includes("s") ? dy : dir.includes("n") ? -dy : 0;
  return { dw, dh };
}

export const resizable: Action<HTMLElement, ResizeOptions> = (node, options) => {
  let opts = options;
  let pointerId: number | null = null;
  let startX = 0;
  let startY = 0;

  function delta(e: PointerEvent): ResizeDelta {
    return resizeDelta(opts.dir, e.clientX - startX, e.clientY - startY);
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
