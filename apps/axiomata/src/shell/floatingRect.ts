/**
 * The geometry of a floating window that can be moved and resized from its edges — the Second Brain's detail window
 * (`docs/plans/orbit-brain.md`, B8) and a movable `Window.svelte` (the Handbook). Pure; sizes are in unscaled units
 * (drawn times the UI scale, `docs/plans/editor-look.md` K10), so a stored rectangle survives a change of the display's
 * density.
 */

import type { ResizeDelta, ResizeDir } from "../canvas/resize";

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export type Edge = "n" | "e" | "s" | "w";

export const MIN_W = 280;
export const MIN_H = 200;

/** The smallest a window may get; each caller may state its own (a dialog wants more room than the detail window). */
export interface MinSize {
  w: number;
  h: number;
}

const DEFAULT_MIN: MinSize = { w: MIN_W, h: MIN_H };

/** How much of a window must stay on screen, so that it can always be taken hold of and pulled back. */
const KEEP_VISIBLE = 80;

/** A size within its limits: at least `min`, at most the screen (`view`, unscaled) — unless the screen is smaller than
 *  the minimum, which then wins. */
export function clampSize(
  size: { w: number; h: number },
  view: { w: number; h: number },
  min: MinSize = DEFAULT_MIN,
): { w: number; h: number } {
  return {
    w: Math.min(Math.max(min.w, size.w), Math.max(min.w, view.w)),
    h: Math.min(Math.max(min.h, size.h), Math.max(min.h, view.h)),
  };
}

/** `rect` with a size within its limits and a position that keeps part of it on screen (`view`: the screen, unscaled). */
export function clampRect(rect: Rect, view: { w: number; h: number }, min: MinSize = DEFAULT_MIN): Rect {
  const { w, h } = clampSize(rect, view, min);
  return {
    w,
    h,
    x: Math.min(Math.max(rect.x, KEEP_VISIBLE - w), view.w - KEEP_VISIBLE),
    y: Math.min(Math.max(rect.y, 0), view.h - KEEP_VISIBLE),
  };
}

export const moved = (rect: Rect, dx: number, dy: number): Rect => ({ ...rect, x: rect.x + dx, y: rect.y + dy });

/**
 * `rect` with one edge pulled by `grow` (positive = the window gets bigger): the right and bottom edges grow it
 * rightwards and downwards; the left and top edges leftwards and upwards, which moves its origin the other way. A pull
 * past the minimum size stops there and does not drag the opposite edge along.
 */
export function resizedEdge(rect: Rect, edge: Edge, grow: number, min: MinSize = DEFAULT_MIN): Rect {
  switch (edge) {
    case "e":
      return { ...rect, w: Math.max(min.w, rect.w + grow) };
    case "s":
      return { ...rect, h: Math.max(min.h, rect.h + grow) };
    case "w": {
      const w = Math.max(min.w, rect.w + grow);
      return { ...rect, w, x: rect.x + rect.w - w };
    }
    case "n": {
      const h = Math.max(min.h, rect.h + grow);
      return { ...rect, h, y: rect.y + rect.h - h };
    }
  }
}

/**
 * `rect` pulled by a resize handle (`canvas/resize.ts`): an edge pulls one side, a corner (`"nw"`, `"se"`, …) the two
 * sides it joins, each by its own axis of `grow` (already in unscaled units, positive = bigger).
 */
export function resizedBy(rect: Rect, dir: ResizeDir, grow: ResizeDelta, min: MinSize = DEFAULT_MIN): Rect {
  let next = rect;
  for (const edge of dir.split("") as Edge[]) {
    next = resizedEdge(next, edge, edge === "e" || edge === "w" ? grow.dw : grow.dh, min);
  }
  return next;
}

/**
 * `resizedBy`, with each pulled edge stopping at the screen's edge (`view`, unscaled): a window pulled bigger stays on
 * screen, and the opposite edge never has to give way for it. Shrinking is never held back, even for a window that
 * already reaches past the screen.
 */
export function resizedWithin(
  rect: Rect,
  dir: ResizeDir,
  grow: ResizeDelta,
  view: { w: number; h: number },
  min: MinSize = DEFAULT_MIN,
): Rect {
  const roomW = dir.includes("w") ? rect.x : dir.includes("e") ? view.w - rect.x - rect.w : 0;
  const roomH = dir.includes("n") ? rect.y : dir.includes("s") ? view.h - rect.y - rect.h : 0;
  const capped = { dw: Math.min(grow.dw, Math.max(0, roomW)), dh: Math.min(grow.dh, Math.max(0, roomH)) };
  return resizedBy(rect, dir, capped, min);
}

/** A stored rectangle, read defensively; `null` when it is not one. */
export function readRect(raw: unknown): Rect | null {
  if (!raw || typeof raw !== "object") return null;
  const { x, y, w, h } = raw as Record<string, unknown>;
  const numbers = [x, y, w, h];
  return numbers.every((n) => typeof n === "number" && Number.isFinite(n))
    ? { x: x as number, y: y as number, w: w as number, h: h as number }
    : null;
}
