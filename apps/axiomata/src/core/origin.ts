/**
 * Where a panel that opens now comes from (`docs/plans/ui-ux-round.md` UX2, "panels open from their origin"): the
 * point the owner just clicked, when the opening follows a click — else the middle of the screen (a key press, an
 * agent handing over a file, a start without any input).
 *
 * The last pointer press is remembered for a moment only: a click from a minute ago did not open the panel an agent
 * opens now.
 */

/** A point on screen, in CSS pixels. */
export interface Point {
  x: number;
  y: number;
}

/** The last input that could have opened something. */
export interface LastInput {
  /** Where the pointer was pressed, or `null` when a key came after it. */
  point: Point | null;
  /** When (ms, `performance.now()`-like clock). */
  at: number;
}

/** How long a press still counts as the cause of an opening: a click → a request → a panel, room for a reply. */
export const ORIGIN_WINDOW_MS = 1500;

/** The point a panel opening at `now` comes from, or `null` for "the middle of the screen". */
export function originAt(last: LastInput | null, now: number): Point | null {
  if (!last?.point) return null;
  return now - last.at <= ORIGIN_WINDOW_MS ? last.point : null;
}

let last: LastInput | null = null;

/** The origin for a panel opening now (see {@link originAt}). */
export function currentOrigin(): Point | null {
  return originAt(last, performance.now());
}

/** Follows the owner's presses and keys; returns a function that stops. */
export function startOriginTracking(): () => void {
  const onPointer = (e: PointerEvent): void => {
    last = { point: { x: e.clientX, y: e.clientY }, at: performance.now() };
  };
  const onKey = (): void => {
    last = { point: null, at: performance.now() };
  };
  window.addEventListener("pointerdown", onPointer, true);
  window.addEventListener("keydown", onKey, true);
  return () => {
    window.removeEventListener("pointerdown", onPointer, true);
    window.removeEventListener("keydown", onKey, true);
  };
}
