/**
 * The geometry of a floating window that can be moved and resized from its edges — the Second Brain's detail window
 * (`docs/plans/orbit-brain.md`, B8). Pure; sizes are in unscaled units (drawn times the UI scale, `docs/plans/editor-look.md`
 * K10), so a stored rectangle survives a change of the display's density.
 */

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export type Edge = "n" | "e" | "s" | "w";

export const MIN_W = 280;
export const MIN_H = 200;

/** How much of a window must stay on screen, so that it can always be taken hold of and pulled back. */
const KEEP_VISIBLE = 80;

/** `rect` with a size within its limits and a position that keeps part of it on screen (`view`: the screen, unscaled). */
export function clampRect(rect: Rect, view: { w: number; h: number }): Rect {
  const w = Math.min(Math.max(MIN_W, rect.w), Math.max(MIN_W, view.w));
  const h = Math.min(Math.max(MIN_H, rect.h), Math.max(MIN_H, view.h));
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
export function resizedEdge(rect: Rect, edge: Edge, grow: number): Rect {
  switch (edge) {
    case "e":
      return { ...rect, w: Math.max(MIN_W, rect.w + grow) };
    case "s":
      return { ...rect, h: Math.max(MIN_H, rect.h + grow) };
    case "w": {
      const w = Math.max(MIN_W, rect.w + grow);
      return { ...rect, w, x: rect.x + rect.w - w };
    }
    case "n": {
      const h = Math.max(MIN_H, rect.h + grow);
      return { ...rect, h, y: rect.y + rect.h - h };
    }
  }
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
