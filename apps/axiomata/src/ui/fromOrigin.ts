/**
 * `transition:fromOrigin` — a panel grows out of whatever opened it and shrinks back into it on close
 * (`docs/plans/ui-ux-round.md` UX2), from the point `core/origin.ts` remembers, or out of the middle of the screen.
 *
 * Follows the motion level (G3): *Full* flies with a little spring, *Calm* is a short plain grow, *Off* only fades.
 * A panel whose resting place is itself a transform (`translateX(-50%)` to centre it) passes that as `base`, so the
 * flight is added to it instead of replacing it.
 */

import { backOut, cubicOut } from "svelte/easing";
import { get } from "svelte/store";
import type { TransitionConfig } from "svelte/transition";

import { motion } from "../core/motion";
import { currentOrigin, type Point } from "../core/origin";

export interface FromOriginParams {
  /** The element's own resting transform, kept under the flight. */
  base?: string;
}

/** The way from the element's centre to the origin (or 0 for the middle of the screen when it sits there). */
export function travel(origin: Point | null, rect: DOMRectReadOnly, viewport: { w: number; h: number }): Point {
  const from = origin ?? { x: viewport.w / 2, y: viewport.h / 2 };
  return { x: from.x - (rect.left + rect.width / 2), y: from.y - (rect.top + rect.height / 2) };
}

/** Where each panel came from, so it closes back into the same place even if the pointer moved on. */
const opened = new WeakMap<Element, Point | null>();

export function fromOrigin(node: Element, params: FromOriginParams = {}): TransitionConfig {
  const base = params.base ?? "";
  const level = get(motion);
  if (level === "off") return { duration: 120, css: (t) => `opacity: ${t}` };

  if (!opened.has(node)) opened.set(node, currentOrigin());
  const way = travel(opened.get(node) ?? null, node.getBoundingClientRect(), {
    w: window.innerWidth,
    h: window.innerHeight,
  });
  const full = level === "full";
  const from = full ? 0.4 : 0.85;
  const reach = full ? 1 : 0.25;
  return {
    duration: full ? 420 : 200,
    easing: full ? backOut : cubicOut,
    css: (t, u) =>
      `opacity: ${Math.min(1, t * 1.6)}; transform: ${base} ` +
      `translate(${way.x * u * reach}px, ${way.y * u * reach}px) scale(${from + (1 - from) * t})`,
  };
}
